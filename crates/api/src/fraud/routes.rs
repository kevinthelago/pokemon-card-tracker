use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    error::AppError,
    models::workspace::{MemberRole, WorkspaceKind},
    notify,
};

use super::flags::{
    FlagKind, FlagSeverity, FlagStatus, KindCount, Notification, RiskFlag,
    RiskSummary, SeverityCount, TargetType, TrendPoint,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/:wid/risk/flags", get(list_flags))
        .route("/workspaces/:wid/risk/flags/bulk", post(bulk_triage))
        .route("/workspaces/:wid/risk/flags/:fid", get(get_flag))
        .route("/workspaces/:wid/risk/flags/:fid", patch(triage_flag))
        .route("/workspaces/:wid/risk/summary", get(get_summary))
        .route("/workspaces/:wid/notifications", get(list_notifications))
        .route("/workspaces/:wid/notifications/:nid/read", post(mark_read))
}

// ─── Request / Response types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListFlagsQuery {
    pub kind: Option<FlagKind>,
    pub severity: Option<FlagSeverity>,
    pub status: Option<FlagStatus>,
    pub target_type: Option<TargetType>,
    pub target_id: Option<Uuid>,
    pub sort_by: Option<SortBy>,
    /// Opaque cursor from a previous page's `next_cursor` field.
    pub after: Option<String>,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Severity,
    CreatedAt,
}

fn default_per_page() -> i64 {
    25
}

#[derive(Debug, Serialize)]
pub struct FlagsPage {
    pub items: Vec<RiskFlag>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TriageBody {
    pub status: TriageStatus,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageStatus {
    Reviewed,
    Dismissed,
}

#[derive(Debug, Deserialize)]
pub struct BulkTriageBody {
    pub flag_ids: Vec<Uuid>,
    pub status: TriageStatus,
}

// ─── Workspace access helpers ────────────────────────────────────────────────

#[derive(FromRow)]
struct WorkspaceAccess {
    kind: WorkspaceKind,
    role: Option<MemberRole>,
}

/// Returns (workspace_kind, member_role). 404 if workspace doesn't exist or
/// caller isn't a member. Collector workspaces only see non-scalper flags.
async fn workspace_access(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    caller_id: Uuid,
) -> Result<WorkspaceAccess, AppError> {
    sqlx::query_as::<_, WorkspaceAccess>(
        r#"
        SELECT w.kind, wm.role
        FROM workspaces w
        LEFT JOIN workspace_members wm
               ON wm.workspace_id = w.id AND wm.user_id = $2
        WHERE w.id = $1
        "#,
    )
    .bind(workspace_id)
    .bind(caller_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
    .and_then(|row| {
        if row.role.is_none() {
            Err(AppError::Forbidden("not a member of this workspace".into()))
        } else {
            Ok(row)
        }
    })
}

fn require_staff_or_owner(access: &WorkspaceAccess) -> Result<(), AppError> {
    match access.role {
        Some(MemberRole::Owner | MemberRole::Staff) => Ok(()),
        _ => Err(AppError::Forbidden("insufficient role".into())),
    }
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// GET /api/workspaces/:wid/risk/flags
async fn list_flags(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<ListFlagsQuery>,
) -> Result<Json<FlagsPage>, AppError> {
    let access = workspace_access(&state.pool, wid, auth.id).await?;

    // Collectors never see scalper flags.
    let effective_kind = if matches!(access.kind, WorkspaceKind::Collector) {
        match q.kind {
            Some(FlagKind::Scalper) => {
                return Ok(Json(FlagsPage { items: vec![], next_cursor: None }));
            }
            _ => q.kind,
        }
    } else {
        q.kind
    };

    let per_page = q.per_page.clamp(1, 100);

    // Decode cursor → (created_at, id) for keyset pagination.
    let cursor_pair: Option<(DateTime<Utc>, Uuid)> = q
        .after
        .as_deref()
        .and_then(decode_cursor);

    // Build the query dynamically. Using runtime queries (no query! macro) so
    // this compiles without DATABASE_URL.
    let mut items: Vec<RiskFlag> = {
        // Sort determines ORDER BY clause and keyset condition column order.
        let sort_sql = match q.sort_by.unwrap_or(SortBy::CreatedAt) {
            SortBy::CreatedAt => "created_at DESC, id DESC",
            // Severity is an ordered enum; cast to int for stable ordering.
            SortBy::Severity => "severity DESC, created_at DESC, id DESC",
        };

        // We assemble filter predicates. sqlx doesn't support dynamic binding
        // counts cleanly, so we use a fixed-parameter approach: pass NULLs for
        // unused filters and check `$n IS NULL OR col = $n` in SQL.
        let cursor_created_at = cursor_pair.as_ref().map(|(ts, _)| *ts);
        let cursor_id = cursor_pair.as_ref().map(|(_, id)| *id);

        let sql = format!(
            r#"
            SELECT id, workspace_id, kind, severity, status,
                   target_type, target_id, title, evidence,
                   reviewed_by, reviewed_at, dismissed_by, dismissed_at,
                   created_at, updated_at
            FROM risk_flags
            WHERE workspace_id = $1
              AND ($2::flag_kind   IS NULL OR kind        = $2)
              AND ($3::flag_severity IS NULL OR severity  = $3)
              AND ($4::flag_status  IS NULL OR status     = $4)
              AND ($5::target_type  IS NULL OR target_type = $5)
              AND ($6::uuid         IS NULL OR target_id  = $6)
              AND (
                  $7::timestamptz IS NULL
                  OR (created_at, id) < ($7, $8)
              )
            ORDER BY {sort_sql}
            LIMIT $9
            "#
        );

        sqlx::query_as::<_, RiskFlag>(&sql)
            .bind(wid)
            .bind(effective_kind)
            .bind(q.severity)
            .bind(q.status)
            .bind(q.target_type)
            .bind(q.target_id)
            .bind(cursor_created_at)
            .bind(cursor_id)
            .bind(per_page + 1)
            .fetch_all(&state.pool)
            .await?
    };

    // Determine if there's a next page.
    let next_cursor = if items.len() as i64 > per_page {
        items.truncate(per_page as usize);
        items.last().map(|f| encode_cursor(f.created_at, f.id))
    } else {
        None
    };

    Ok(Json(FlagsPage { items, next_cursor }))
}

/// GET /api/workspaces/:wid/risk/flags/:fid
async fn get_flag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, fid)): Path<(Uuid, Uuid)>,
) -> Result<Json<RiskFlag>, AppError> {
    let access = workspace_access(&state.pool, wid, auth.id).await?;

    let flag = sqlx::query_as::<_, RiskFlag>(
        r#"
        SELECT id, workspace_id, kind, severity, status,
               target_type, target_id, title, evidence,
               reviewed_by, reviewed_at, dismissed_by, dismissed_at,
               created_at, updated_at
        FROM risk_flags
        WHERE id = $1 AND workspace_id = $2
        "#,
    )
    .bind(fid)
    .bind(wid)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    // Collectors can't see scalper flags.
    if matches!(access.kind, WorkspaceKind::Collector)
        && matches!(flag.kind, FlagKind::Scalper)
    {
        return Err(AppError::NotFound);
    }

    Ok(Json(flag))
}

/// PATCH /api/workspaces/:wid/risk/flags/:fid
async fn triage_flag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, fid)): Path<(Uuid, Uuid)>,
    Json(body): Json<TriageBody>,
) -> Result<Json<RiskFlag>, AppError> {
    let access = workspace_access(&state.pool, wid, auth.id).await?;
    require_staff_or_owner(&access)?;

    let flag = apply_triage(&state.pool, wid, fid, auth.id, body.status).await?;

    write_audit_log(
        &state.pool,
        wid,
        auth.id,
        triage_action_name(body.status),
        "risk_flag",
        fid,
        json!({"new_status": format!("{:?}", body.status).to_lowercase()}),
    )
    .await?;

    // Scalper-dismissal tuning hook: emit a note for the detect-scalpers stream.
    if matches!(body.status, TriageStatus::Dismissed)
        && matches!(flag.kind, FlagKind::Scalper)
    {
        tracing::info!(
            flag_id = %fid,
            workspace_id = %wid,
            "scalper flag dismissed — tuning feedback opportunity"
        );
    }

    Ok(Json(flag))
}

/// POST /api/workspaces/:wid/risk/flags/bulk
async fn bulk_triage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Json(body): Json<BulkTriageBody>,
) -> Result<StatusCode, AppError> {
    let access = workspace_access(&state.pool, wid, auth.id).await?;
    require_staff_or_owner(&access)?;

    if body.flag_ids.is_empty() {
        return Err(AppError::BadRequest("flag_ids must not be empty".into()));
    }
    if body.flag_ids.len() > 100 {
        return Err(AppError::BadRequest("at most 100 flags per bulk operation".into()));
    }

    for &fid in &body.flag_ids {
        let flag = apply_triage(&state.pool, wid, fid, auth.id, body.status).await?;
        write_audit_log(
            &state.pool,
            wid,
            auth.id,
            triage_action_name(body.status),
            "risk_flag",
            fid,
            json!({"bulk": true, "new_status": format!("{:?}", body.status).to_lowercase()}),
        )
        .await?;

        if matches!(body.status, TriageStatus::Dismissed)
            && matches!(flag.kind, FlagKind::Scalper)
        {
            tracing::info!(
                flag_id = %fid,
                workspace_id = %wid,
                "scalper flag dismissed via bulk — tuning feedback opportunity"
            );
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/workspaces/:wid/risk/summary
async fn get_summary(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
) -> Result<Json<RiskSummary>, AppError> {
    let access = workspace_access(&state.pool, wid, auth.id).await?;

    // Collector workspaces exclude scalper from counts.
    let exclude_scalper = matches!(access.kind, WorkspaceKind::Collector);

    let counts_by_kind: Vec<KindCount> = sqlx::query_as::<_, KindCount>(
        r#"
        SELECT kind,
               COUNT(*) FILTER (WHERE status = 'open')  AS open,
               COUNT(*)                                  AS total
        FROM risk_flags
        WHERE workspace_id = $1
          AND ($2 = false OR kind != 'scalper')
        GROUP BY kind
        ORDER BY kind
        "#,
    )
    .bind(wid)
    .bind(exclude_scalper)
    .fetch_all(&state.pool)
    .await?;

    let counts_by_severity: Vec<SeverityCount> = sqlx::query_as::<_, SeverityCount>(
        r#"
        SELECT severity,
               COUNT(*) FILTER (WHERE status = 'open')  AS open,
               COUNT(*)                                  AS total
        FROM risk_flags
        WHERE workspace_id = $1
          AND ($2 = false OR kind != 'scalper')
        GROUP BY severity
        ORDER BY severity DESC
        "#,
    )
    .bind(wid)
    .bind(exclude_scalper)
    .fetch_all(&state.pool)
    .await?;

    // Daily open-vs-resolved trend for the last 30 days.
    let trend: Vec<TrendPoint> = sqlx::query_as::<_, TrendPoint>(
        r#"
        SELECT
            date_trunc('day', gs.day) AS day,
            COUNT(f.id) FILTER (WHERE f.created_at >= gs.day
                                  AND f.created_at <  gs.day + INTERVAL '1 day') AS opened,
            COUNT(f.id) FILTER (WHERE f.status IN ('reviewed','dismissed')
                                  AND f.updated_at >= gs.day
                                  AND f.updated_at <  gs.day + INTERVAL '1 day') AS resolved
        FROM generate_series(
            NOW() - INTERVAL '29 days',
            NOW(),
            INTERVAL '1 day'
        ) AS gs(day)
        LEFT JOIN risk_flags f ON f.workspace_id = $1
            AND ($2 = false OR f.kind != 'scalper')
        GROUP BY gs.day
        ORDER BY gs.day ASC
        "#,
    )
    .bind(wid)
    .bind(exclude_scalper)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(RiskSummary { counts_by_kind, counts_by_severity, trend }))
}

/// GET /api/workspaces/:wid/notifications
async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
) -> Result<Json<Vec<Notification>>, AppError> {
    workspace_access(&state.pool, wid, auth.id).await?;

    let rows = sqlx::query_as::<_, Notification>(
        r#"
        SELECT id, workspace_id, user_id, kind, title, body, deep_link, read_at, created_at
        FROM notifications
        WHERE workspace_id = $1 AND user_id = $2
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .bind(wid)
    .bind(auth.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// POST /api/workspaces/:wid/notifications/:nid/read
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, nid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    workspace_access(&state.pool, wid, auth.id).await?;

    let updated = sqlx::query(
        r#"
        UPDATE notifications
           SET read_at = NOW()
         WHERE id = $1 AND workspace_id = $2 AND user_id = $3 AND read_at IS NULL
        "#,
    )
    .bind(nid)
    .bind(wid)
    .bind(auth.id)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

async fn apply_triage(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    flag_id: Uuid,
    actor_id: Uuid,
    status: TriageStatus,
) -> Result<RiskFlag, AppError> {
    let (new_status, actor_col, ts_col): (FlagStatus, &str, &str) = match status {
        TriageStatus::Reviewed => (FlagStatus::Reviewed, "reviewed_by", "reviewed_at"),
        TriageStatus::Dismissed => (FlagStatus::Dismissed, "dismissed_by", "dismissed_at"),
    };

    let sql = format!(
        r#"
        UPDATE risk_flags
           SET status      = $1,
               {actor_col} = $2,
               {ts_col}    = NOW(),
               updated_at  = NOW()
         WHERE id = $3 AND workspace_id = $4 AND status = 'open'
        RETURNING id, workspace_id, kind, severity, status,
                  target_type, target_id, title, evidence,
                  reviewed_by, reviewed_at, dismissed_by, dismissed_at,
                  created_at, updated_at
        "#
    );

    sqlx::query_as::<_, RiskFlag>(&sql)
        .bind(new_status)
        .bind(actor_id)
        .bind(flag_id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

async fn write_audit_log(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    actor_id: Uuid,
    action: &str,
    entity_type: &str,
    entity_id: Uuid,
    meta: serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_log (workspace_id, actor_id, action, entity_type, entity_id, meta)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(workspace_id)
    .bind(actor_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(meta)
    .execute(pool)
    .await?;
    Ok(())
}

fn triage_action_name(s: TriageStatus) -> &'static str {
    match s {
        TriageStatus::Reviewed => "flag_reviewed",
        TriageStatus::Dismissed => "flag_dismissed",
    }
}

// ─── Cursor encoding (created_at epoch_ms:uuid) ──────────────────────────────

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let raw = format!("{}:{}", created_at.timestamp_millis(), id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let s = std::str::from_utf8(&bytes).ok()?;
    let (ts_str, id_str) = s.split_once(':')?;
    let ms: i64 = ts_str.parse().ok()?;
    let dt = DateTime::from_timestamp_millis(ms)?;
    let id = Uuid::parse_str(id_str).ok()?;
    Some((dt, id))
}

// ─── Public helper: create a flag + dispatch notifications ───────────────────

/// Called by detect-stolen-cards and detect-scalpers streams to insert a flag
/// and fan out in-app + email notifications to all workspace members.
pub async fn create_flag(
    state: &AppState,
    workspace_id: Uuid,
    kind: FlagKind,
    severity: FlagSeverity,
    target_type: TargetType,
    target_id: Uuid,
    title: &str,
    evidence: serde_json::Value,
) -> Result<RiskFlag, AppError> {
    let flag = sqlx::query_as::<_, RiskFlag>(
        r#"
        INSERT INTO risk_flags (workspace_id, kind, severity, target_type, target_id, title, evidence)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, workspace_id, kind, severity, status,
                  target_type, target_id, title, evidence,
                  reviewed_by, reviewed_at, dismissed_by, dismissed_at,
                  created_at, updated_at
        "#,
    )
    .bind(workspace_id)
    .bind(kind)
    .bind(severity)
    .bind(target_type)
    .bind(target_id)
    .bind(title)
    .bind(evidence)
    .fetch_one(&state.pool)
    .await?;

    let deep_link = format!("/workspaces/{workspace_id}/risk#{}", flag.id);
    notify::dispatch_new_flag(state, &flag, &deep_link).await;

    Ok(flag)
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips() {
        let id = Uuid::new_v4();
        let ts = Utc::now();
        // Truncate to millisecond precision to match encode/decode.
        let ts = DateTime::from_timestamp_millis(ts.timestamp_millis()).unwrap();
        let encoded = encode_cursor(ts, id);
        let (decoded_ts, decoded_id) = decode_cursor(&encoded).unwrap();
        assert_eq!(decoded_id, id);
        assert_eq!(decoded_ts, ts);
    }

    #[test]
    fn cursor_invalid_returns_none() {
        assert!(decode_cursor("not-valid-base64!!!").is_none());
        assert!(decode_cursor("aGVsbG8=").is_none()); // valid b64 but wrong format
    }

    #[test]
    fn flag_severity_ordering() {
        assert!(FlagSeverity::Critical > FlagSeverity::High);
        assert!(FlagSeverity::High > FlagSeverity::Medium);
        assert!(FlagSeverity::Medium > FlagSeverity::Low);
    }

    #[test]
    fn triage_status_action_names() {
        assert_eq!(triage_action_name(TriageStatus::Reviewed), "flag_reviewed");
        assert_eq!(triage_action_name(TriageStatus::Dismissed), "flag_dismissed");
    }
}
