//! Stolen-card reporting, moderation, and cert-match detection.
//!
//! # Endpoints (all under `/api`)
//!
//! ## Shared community reports
//!   POST   /stolen/reports               — submit (authenticated)
//!   GET    /stolen/reports               — list my reports (authenticated)
//!   GET    /stolen/reports/:id           — get one report
//!   POST   /stolen/reports/:id/confirm   — confirm (moderator only)
//!   POST   /stolen/reports/:id/reject    — reject  (moderator only)
//!   POST   /stolen/reports/:id/dispute   — dispute a report
//!   GET    /stolen/shared                — confirmed cert list (no PII)
//!   GET    /stolen/queue                 — moderation queue (moderator only)
//!
//! ## Per-workspace private list
//!   POST   /workspaces/:wid/stolen/private        — add to private list
//!   GET    /workspaces/:wid/stolen/private        — list private entries
//!   DELETE /workspaces/:wid/stolen/private/:id   — remove entry
//!   POST   /workspaces/:wid/stolen/private/:id/resolve — mark recovered
//!
//! # Public cross-stream API
//!   [`check_and_flag_stolen`]    — called by catalogue/grading on card add
//!   [`rescan_workspace_stolen`]  — rescans all instances after a new confirmation

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{app::AppState, auth::AuthUser, error::AppError};

// ─── Request DTOs ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateReportBody {
    grader: String,
    cert_number: String,
    evidence: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModeratorNoteBody {
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DisputeBody {
    reason: String,
}

#[derive(Debug, Deserialize)]
struct AddPrivateBody {
    grader: String,
    cert_number: String,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PageParams {
    limit: Option<i64>,
    offset: Option<i64>,
}

// ─── Response DTOs ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StolenReportDto {
    pub id: Uuid,
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub evidence: Option<String>,
    pub notes: Option<String>,
    pub moderator_notes: Option<String>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SharedCertDto {
    pub grader: String,
    pub cert_number: String,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PrivateStolenDto {
    pub id: Uuid,
    pub grader: String,
    pub cert_number: String,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ListDto<T> {
    pub items: Vec<T>,
    pub total: i64,
}

// ─── Route builder ────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stolen/reports", post(submit_report).get(list_my_reports))
        .route("/stolen/reports/:id", get(get_report))
        .route("/stolen/reports/:id/confirm", post(confirm_report))
        .route("/stolen/reports/:id/reject", post(reject_report))
        .route("/stolen/reports/:id/dispute", post(dispute_report))
        .route("/stolen/shared", get(list_shared))
        .route("/stolen/queue", get(list_queue))
        .route(
            "/workspaces/:wid/stolen/private",
            post(add_private).get(list_private),
        )
        .route(
            "/workspaces/:wid/stolen/private/:id",
            delete(remove_private),
        )
        .route(
            "/workspaces/:wid/stolen/private/:id/resolve",
            post(resolve_private),
        )
}

// ─── Guards ───────────────────────────────────────────────────────────────────

fn valid_grader(g: &str) -> bool {
    matches!(g, "PSA" | "CGC" | "BGS")
}

async fn require_moderator(pool: &sqlx::PgPool, user_id: Uuid) -> Result<(), AppError> {
    let is_mod: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM platform_moderators WHERE user_id = $1)")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)?;

    if !is_mod {
        return Err(AppError::Forbidden("moderator access required".into()));
    }
    Ok(())
}

async fn require_workspace_member(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), AppError> {
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_members WHERE user_id = $1 AND workspace_id = $2)",
    )
    .bind(user_id)
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)?;

    if !is_member {
        return Err(AppError::Forbidden("not a member of this workspace".into()));
    }
    Ok(())
}

fn row_to_report_dto(row: &sqlx::postgres::PgRow) -> StolenReportDto {
    StolenReportDto {
        id: row.get("id"),
        grader: row.get("grader"),
        cert_number: row.get("cert_number"),
        status: row.get("status"),
        evidence: row.get("evidence"),
        notes: row.get("notes"),
        moderator_notes: row.get("moderator_notes"),
        confirmed_at: row.get("confirmed_at"),
        created_at: row.get("created_at"),
    }
}

fn row_to_private_dto(row: &sqlx::postgres::PgRow) -> PrivateStolenDto {
    let resolved_at: Option<DateTime<Utc>> = row.get("resolved_at");
    PrivateStolenDto {
        id: row.get("id"),
        grader: row.get("grader"),
        cert_number: row.get("cert_number"),
        notes: row.get("notes"),
        is_active: resolved_at.is_none(),
        created_at: row.get("created_at"),
    }
}

// ─── Handlers: shared community reports ──────────────────────────────────────

async fn submit_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateReportBody>,
) -> Result<(StatusCode, Json<StolenReportDto>), AppError> {
    let grader = body.grader.trim().to_uppercase();
    let cert_number = body.cert_number.trim().to_string();

    if !valid_grader(&grader) {
        return Err(AppError::BadRequest(format!("unknown grader '{grader}'")));
    }
    if cert_number.is_empty() {
        return Err(AppError::BadRequest("cert_number is required".into()));
    }

    let evidence = body
        .evidence
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let notes = body
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let row = sqlx::query(
        r#"
        INSERT INTO stolen_reports (grader, cert_number, reporter_user_id, evidence, notes)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, grader, cert_number, status, evidence, notes,
                  moderator_notes, confirmed_at, created_at
        "#,
    )
    .bind(&grader)
    .bind(&cert_number)
    .bind(auth.id)
    .bind(evidence)
    .bind(notes)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db) if db.constraint() == Some("idx_stolen_reports_no_dupe") => {
            AppError::Conflict("you have already reported this cert".into())
        }
        other => AppError::from(other),
    })?;

    Ok((StatusCode::CREATED, Json(row_to_report_dto(&row))))
}

async fn list_my_reports(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(page): Query<PageParams>,
) -> Result<Json<ListDto<StolenReportDto>>, AppError> {
    let limit = page.limit.unwrap_or(25).min(100);
    let offset = page.offset.unwrap_or(0);

    let rows = sqlx::query(
        r#"
        SELECT id, grader, cert_number, status, evidence, notes,
               moderator_notes, confirmed_at, created_at
          FROM stolen_reports
         WHERE reporter_user_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3
        "#,
    )
    .bind(auth.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM stolen_reports WHERE reporter_user_id = $1")
            .bind(auth.id)
            .fetch_one(&state.pool)
            .await?;

    Ok(Json(ListDto {
        items: rows.iter().map(row_to_report_dto).collect(),
        total,
    }))
}

async fn get_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<StolenReportDto>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, grader, cert_number, reporter_user_id, status, evidence, notes,
               moderator_notes, confirmed_at, created_at
          FROM stolen_reports
         WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let reporter_user_id: Uuid = row.get("reporter_user_id");
    if reporter_user_id != auth.id {
        require_moderator(&state.pool, auth.id).await?;
    }

    Ok(Json(row_to_report_dto(&row)))
}

async fn confirm_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ModeratorNoteBody>,
) -> Result<Json<StolenReportDto>, AppError> {
    require_moderator(&state.pool, auth.id).await?;

    let mod_notes = body
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let row = sqlx::query(
        r#"
        UPDATE stolen_reports
           SET status          = 'confirmed',
               moderator_notes = COALESCE($2, moderator_notes),
               confirmed_at    = NOW(),
               updated_at      = NOW()
         WHERE id = $1
        RETURNING id, grader, cert_number, status, evidence, notes,
                  moderator_notes, confirmed_at, created_at
        "#,
    )
    .bind(id)
    .bind(mod_notes)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let _ = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, entity_type, entity_id) VALUES ($1, 'confirm', 'stolen_report', $2)",
    )
    .bind(auth.id)
    .bind(id)
    .execute(&state.pool)
    .await;

    Ok(Json(row_to_report_dto(&row)))
}

async fn reject_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ModeratorNoteBody>,
) -> Result<Json<StolenReportDto>, AppError> {
    require_moderator(&state.pool, auth.id).await?;

    let mod_notes = body
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let row = sqlx::query(
        r#"
        UPDATE stolen_reports
           SET status          = 'rejected',
               moderator_notes = COALESCE($2, moderator_notes),
               updated_at      = NOW()
         WHERE id = $1
        RETURNING id, grader, cert_number, status, evidence, notes,
                  moderator_notes, confirmed_at, created_at
        "#,
    )
    .bind(id)
    .bind(mod_notes)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let _ = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, entity_type, entity_id) VALUES ($1, 'reject', 'stolen_report', $2)",
    )
    .bind(auth.id)
    .bind(id)
    .execute(&state.pool)
    .await;

    Ok(Json(row_to_report_dto(&row)))
}

async fn dispute_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DisputeBody>,
) -> Result<Json<StolenReportDto>, AppError> {
    let reason = body.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }

    let existing = sqlx::query("SELECT status FROM stolen_reports WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let status: String = existing.get("status");
    if !matches!(status.as_str(), "pending" | "confirmed") {
        return Err(AppError::Conflict(format!(
            "cannot dispute a report with status '{status}'"
        )));
    }

    sqlx::query(
        "INSERT INTO dispute_records (stolen_report_id, filed_by_user_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(auth.id)
    .bind(reason)
    .execute(&state.pool)
    .await?;

    let row = sqlx::query(
        r#"
        UPDATE stolen_reports
           SET status     = 'disputed',
               updated_at = NOW()
         WHERE id = $1
        RETURNING id, grader, cert_number, status, evidence, notes,
                  moderator_notes, confirmed_at, created_at
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row_to_report_dto(&row)))
}

async fn list_shared(
    State(state): State<AppState>,
    Query(page): Query<PageParams>,
) -> Result<Json<ListDto<SharedCertDto>>, AppError> {
    let limit = page.limit.unwrap_or(100).min(500);
    let offset = page.offset.unwrap_or(0);

    let rows = sqlx::query(
        r#"
        SELECT grader, cert_number, confirmed_at
          FROM stolen_reports
         WHERE status = 'confirmed'
         ORDER BY confirmed_at DESC
         LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM stolen_reports WHERE status = 'confirmed'")
            .fetch_one(&state.pool)
            .await?;

    Ok(Json(ListDto {
        items: rows
            .iter()
            .map(|r| SharedCertDto {
                grader: r.get("grader"),
                cert_number: r.get("cert_number"),
                confirmed_at: r.get("confirmed_at"),
            })
            .collect(),
        total,
    }))
}

async fn list_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(page): Query<PageParams>,
) -> Result<Json<ListDto<StolenReportDto>>, AppError> {
    require_moderator(&state.pool, auth.id).await?;

    let limit = page.limit.unwrap_or(25).min(100);
    let offset = page.offset.unwrap_or(0);

    let rows = sqlx::query(
        r#"
        SELECT id, grader, cert_number, status, evidence, notes,
               moderator_notes, confirmed_at, created_at
          FROM stolen_reports
         WHERE status IN ('pending', 'disputed')
         ORDER BY created_at ASC
         LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stolen_reports WHERE status IN ('pending', 'disputed')",
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(ListDto {
        items: rows.iter().map(row_to_report_dto).collect(),
        total,
    }))
}

// ─── Handlers: private workspace list ─────────────────────────────────────────

async fn add_private(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Json(body): Json<AddPrivateBody>,
) -> Result<(StatusCode, Json<PrivateStolenDto>), AppError> {
    require_workspace_member(&state.pool, auth.id, wid).await?;

    let grader = body.grader.trim().to_uppercase();
    let cert_number = body.cert_number.trim().to_string();
    if !valid_grader(&grader) {
        return Err(AppError::BadRequest(format!("unknown grader '{grader}'")));
    }
    if cert_number.is_empty() {
        return Err(AppError::BadRequest("cert_number is required".into()));
    }

    let notes = body
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let row = sqlx::query(
        r#"
        INSERT INTO private_stolen_certs
            (workspace_id, grader, cert_number, added_by_user_id, notes)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, grader, cert_number, notes, resolved_at, created_at
        "#,
    )
    .bind(wid)
    .bind(&grader)
    .bind(&cert_number)
    .bind(auth.id)
    .bind(notes)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db) if db.constraint() == Some("uq_private_stolen_cert") => {
            AppError::Conflict("this cert is already on your workspace stolen list".into())
        }
        other => AppError::from(other),
    })?;

    Ok((StatusCode::CREATED, Json(row_to_private_dto(&row))))
}

async fn list_private(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Query(page): Query<PageParams>,
) -> Result<Json<ListDto<PrivateStolenDto>>, AppError> {
    require_workspace_member(&state.pool, auth.id, wid).await?;

    let limit = page.limit.unwrap_or(50).min(200);
    let offset = page.offset.unwrap_or(0);

    let rows = sqlx::query(
        r#"
        SELECT id, grader, cert_number, notes, resolved_at, created_at
          FROM private_stolen_certs
         WHERE workspace_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3
        "#,
    )
    .bind(wid)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM private_stolen_certs WHERE workspace_id = $1")
            .bind(wid)
            .fetch_one(&state.pool)
            .await?;

    Ok(Json(ListDto {
        items: rows.iter().map(row_to_private_dto).collect(),
        total,
    }))
}

async fn remove_private(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_workspace_member(&state.pool, auth.id, wid).await?;

    let result =
        sqlx::query("DELETE FROM private_stolen_certs WHERE id = $1 AND workspace_id = $2")
            .bind(id)
            .bind(wid)
            .execute(&state.pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn resolve_private(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<PrivateStolenDto>, AppError> {
    require_workspace_member(&state.pool, auth.id, wid).await?;

    let row = sqlx::query(
        r#"
        UPDATE private_stolen_certs
           SET resolved_at = NOW()
         WHERE id = $1 AND workspace_id = $2 AND resolved_at IS NULL
        RETURNING id, grader, cert_number, notes, resolved_at, created_at
        "#,
    )
    .bind(id)
    .bind(wid)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(row_to_private_dto(&row)))
}

// ─── Cross-stream public API ──────────────────────────────────────────────────

/// Returns true if the given cert appears on the shared confirmed list or on
/// the workspace's private active list. Called by the catalogue/grading stream.
#[allow(dead_code)]
pub async fn check_and_flag_stolen(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    grader: &str,
    cert_number: &str,
) -> anyhow::Result<bool> {
    let on_shared: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM stolen_reports WHERE grader = $1 AND cert_number = $2 AND status = 'confirmed')",
    )
    .bind(grader)
    .bind(cert_number)
    .fetch_one(pool)
    .await?;

    if on_shared {
        return Ok(true);
    }

    let on_private: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM private_stolen_certs WHERE workspace_id = $1 AND grader = $2 AND cert_number = $3 AND resolved_at IS NULL)",
    )
    .bind(workspace_id)
    .bind(grader)
    .bind(cert_number)
    .fetch_one(pool)
    .await?;

    Ok(on_private)
}

/// Rescans all card instances in a workspace and inserts risk_flags for any
/// that match the stolen lists but aren't already flagged.
///
/// Triggered when a new confirmed stolen report is added.
#[allow(dead_code)]
pub async fn rescan_workspace_stolen(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"
        WITH stolen AS (
            SELECT grader, cert_number FROM stolen_reports WHERE status = 'confirmed'
            UNION ALL
            SELECT grader, cert_number FROM private_stolen_certs
             WHERE workspace_id = $1 AND resolved_at IS NULL
        )
        INSERT INTO risk_flags
            (workspace_id, kind, severity, target_type, target_id, status, detail)
        SELECT
            $1,
            'stolen',
            'high',
            'instance',
            ci.id,
            'open',
            jsonb_build_object('grader', ci.grader, 'cert_number', ci.cert_number)
        FROM card_instances ci
        JOIN stolen s ON s.grader = ci.grader AND s.cert_number = ci.cert_number
        WHERE ci.workspace_id = $1
          AND NOT EXISTS (
              SELECT 1 FROM risk_flags rf
               WHERE rf.target_type = 'instance'
                 AND rf.target_id   = ci.id
                 AND rf.kind        = 'stolen'
                 AND rf.status      = 'open'
          )
        "#,
    )
    .bind(workspace_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_grader_accepts_known() {
        assert!(valid_grader("PSA"));
        assert!(valid_grader("CGC"));
        assert!(valid_grader("BGS"));
    }

    #[test]
    fn valid_grader_rejects_unknown() {
        assert!(!valid_grader("psa"));
        assert!(!valid_grader("SGC"));
        assert!(!valid_grader(""));
        assert!(!valid_grader("NONE"));
    }
}
