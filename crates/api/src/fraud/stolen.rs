//! Stolen-card reporting, moderation, and cert-match detection.
//!
//! # Endpoints (all under `/api/v1`)
//! ## Shared-list reporting
//!   `POST   /stolen/reports`               Submit a report for the community list
//!   `GET    /stolen/reports`               List my submitted reports
//!   `GET    /stolen/reports/:id`           Get a single report
//!   `POST   /stolen/reports/:id/confirm`   Confirm (platform-moderator only)
//!   `POST   /stolen/reports/:id/reject`    Reject  (platform-moderator only)
//!   `POST   /stolen/reports/:id/dispute`   Dispute (accused owner)
//!   `POST   /stolen/reports/:id/resolve`   Mark resolved/recovered (platform-moderator)
//!
//! ## Read-only shared list (cert# + status, no PII)
//!   `GET    /stolen/shared`               Confirmed stolen-cert list (public within authn)
//!   `GET    /stolen/queue`                Moderation queue (platform-moderator only)
//!
//! ## Per-workspace private list (always trusted, no moderation)
//!   `POST   /stolen/private`              Add a cert to this workspace's private list
//!   `GET    /stolen/private`              List this workspace's private entries
//!   `DELETE /stolen/private/:id`          Remove from private list
//!   `POST   /stolen/private/:id/resolve`  Mark a private entry as recovered
//!
//! # Public API used by other streams
//!   [`check_and_flag_stolen`] — called by `catalogue` and `grading` on every add/verify
//!   [`rescan_workspace_stolen`] — called directly or via [`StolenRescanJob`]

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::Claims,
    error::{ApiError, Result},
};

// ─── DB row ───────────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct StolenReportRow {
    id: Uuid,
    grader: String,
    cert_number: String,
    reporter_user_id: Uuid,
    workspace_id: Option<Uuid>,
    scope: String,
    status: String,
    evidence: Option<String>,
    reporter_notes: Option<String>,
    moderator_notes: Option<String>,
    dispute_user_id: Option<Uuid>,
    dispute_notes: Option<String>,
    dispute_at: Option<DateTime<Utc>>,
    confirmed_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

// ─── Request / response DTOs ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    pub grader: String,
    pub cert_number: String,
    /// Optional evidence description / photo reference
    pub evidence: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddPrivateRequest {
    pub grader: String,
    pub cert_number: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DisputeRequest {
    /// Required: the disputing owner must explain their position
    pub notes: String,
}

#[derive(Debug, Deserialize)]
pub struct ModeratorActionRequest {
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    /// Cursor is the `id` of the last record from the previous page
    pub cursor: Option<Uuid>,
}

/// Full report DTO — returned to the reporter or moderator (includes PII).
#[derive(Debug, Serialize)]
pub struct StolenReportDto {
    pub id: Uuid,
    pub grader: String,
    pub cert_number: String,
    pub scope: String,
    pub status: String,
    pub evidence: Option<String>,
    pub notes: Option<String>,
    pub moderator_notes: Option<String>,
    pub dispute_notes: Option<String>,
    pub dispute_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Shared-list DTO — only cert# and status, never PII.
#[derive(Debug, Serialize)]
pub struct SharedCertDto {
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<Uuid>,
}

// ─── Routes ──────────────────────────────────────────────────────────────────

/// Returns the Axum sub-router for all stolen-card endpoints.
///
/// Mount this under `/api/v1` in `crates/api/src/app.rs`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Shared-list reports
        .route("/stolen/reports", post(create_report).get(list_my_reports))
        .route("/stolen/reports/:id", get(get_report))
        .route("/stolen/reports/:id/confirm", post(confirm_report))
        .route("/stolen/reports/:id/reject", post(reject_report))
        .route("/stolen/reports/:id/dispute", post(dispute_report))
        .route("/stolen/reports/:id/resolve", post(resolve_shared_report))
        // Read-only shared list
        .route("/stolen/shared", get(list_shared))
        .route("/stolen/queue", get(list_queue))
        // Private workspace list
        .route("/stolen/private", post(add_private).get(list_private))
        .route("/stolen/private/:id", delete(remove_private))
        .route("/stolen/private/:id/resolve", post(resolve_private))
}

// ─── Handlers — shared list ───────────────────────────────────────────────────

async fn create_report(
    State(state): State<AppState>,
    claims: Claims,
    Json(body): Json<CreateReportRequest>,
) -> Result<impl IntoResponse> {
    let (grader, cert_number) = normalize_cert(&body.grader, &body.cert_number)?;

    let report = db_upsert_report(
        &state.db,
        &grader,
        &cert_number,
        claims.user_id,
        None, // shared scope: no workspace_id
        body.evidence.as_deref(),
        body.notes.as_deref(),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(to_dto(report))))
}

async fn list_my_reports(
    State(state): State<AppState>,
    claims: Claims,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse> {
    let limit = clamp_limit(q.limit);
    let rows = db_list_by_reporter(&state.db, claims.user_id, limit, q.cursor).await?;
    Ok(Json(paginate(rows, limit)))
}

async fn get_report(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("report"))?;

    // Only the reporter (original or secondary) or a moderator may see full details.
    let is_reporter = db_is_reporter(&state.db, id, claims.user_id).await?;
    if !is_reporter && !claims.is_platform_moderator {
        return Err(ApiError::forbidden());
    }

    Ok(Json(to_dto(row)))
}

async fn confirm_report(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<ModeratorActionRequest>,
) -> Result<impl IntoResponse> {
    require_moderator(&claims)?;

    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("report"))?;

    if !matches!(row.status.as_str(), "pending" | "disputed") {
        return Err(ApiError::validation(
            "only pending or disputed reports can be confirmed",
        ));
    }

    let updated = db_transition_report(
        &state.db,
        id,
        "confirmed",
        Some(Utc::now()),
        None,
        body.notes.as_deref(),
    )
    .await?;

    write_audit_log(
        &state.db,
        claims.user_id,
        "confirm_stolen_report",
        "stolen_report",
        id,
        serde_json::json!({ "notes": body.notes }),
    )
    .await?;

    // Trigger a background rescan of all workspaces now that the stolen list grew.
    enqueue_rescan(&state.db, id).await?;

    Ok(Json(to_dto(updated)))
}

async fn reject_report(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<ModeratorActionRequest>,
) -> Result<impl IntoResponse> {
    require_moderator(&claims)?;

    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("report"))?;

    if !matches!(row.status.as_str(), "pending" | "disputed") {
        return Err(ApiError::validation(
            "only pending or disputed reports can be rejected",
        ));
    }

    let updated = db_transition_report(
        &state.db,
        id,
        "rejected",
        None,
        None,
        body.notes.as_deref(),
    )
    .await?;

    write_audit_log(
        &state.db,
        claims.user_id,
        "reject_stolen_report",
        "stolen_report",
        id,
        serde_json::json!({ "notes": body.notes }),
    )
    .await?;

    Ok(Json(to_dto(updated)))
}

async fn dispute_report(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<DisputeRequest>,
) -> Result<impl IntoResponse> {
    if body.notes.trim().is_empty() {
        return Err(ApiError::validation("dispute notes are required"));
    }

    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("report"))?;

    if !matches!(row.status.as_str(), "pending" | "confirmed") {
        return Err(ApiError::validation(
            "report cannot be disputed in its current state",
        ));
    }

    let updated = db_dispute_report(&state.db, id, claims.user_id, &body.notes).await?;

    write_audit_log(
        &state.db,
        claims.user_id,
        "dispute_stolen_report",
        "stolen_report",
        id,
        serde_json::json!({ "dispute_notes": body.notes }),
    )
    .await?;

    Ok(Json(to_dto(updated)))
}

async fn resolve_shared_report(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
    Json(body): Json<ModeratorActionRequest>,
) -> Result<impl IntoResponse> {
    require_moderator(&claims)?;

    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("report"))?;

    if row.status != "confirmed" {
        return Err(ApiError::validation(
            "only confirmed reports can be marked resolved",
        ));
    }

    let updated = db_transition_report(
        &state.db,
        id,
        "resolved",
        None,
        Some(Utc::now()),
        body.notes.as_deref(),
    )
    .await?;

    write_audit_log(
        &state.db,
        claims.user_id,
        "resolve_stolen_report",
        "stolen_report",
        id,
        serde_json::json!({ "notes": body.notes }),
    )
    .await?;

    Ok(Json(to_dto(updated)))
}

async fn list_shared(
    State(state): State<AppState>,
    _claims: Claims,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse> {
    let limit = clamp_limit(q.limit);

    let rows: Vec<StolenReportRow> = sqlx::query_as(
        "SELECT id, grader, cert_number, reporter_user_id, workspace_id, \
                scope, status, evidence, reporter_notes, moderator_notes, \
                dispute_user_id, dispute_notes, dispute_at, \
                confirmed_at, resolved_at, created_at, updated_at \
         FROM stolen_reports \
         WHERE scope = 'shared' AND status = 'confirmed' \
           AND ($1::uuid IS NULL OR id < $1) \
         ORDER BY confirmed_at DESC \
         LIMIT $2",
    )
    .bind(q.cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let next_cursor = if rows.len() as i64 == limit {
        rows.last().map(|r| r.id)
    } else {
        None
    };

    let data: Vec<SharedCertDto> = rows
        .into_iter()
        .map(|r| SharedCertDto {
            grader: r.grader,
            cert_number: r.cert_number,
            status: r.status,
            confirmed_at: r.confirmed_at,
        })
        .collect();

    Ok(Json(ListResponse { data, next_cursor }))
}

async fn list_queue(
    State(state): State<AppState>,
    claims: Claims,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse> {
    require_moderator(&claims)?;
    let limit = clamp_limit(q.limit);
    let rows = db_list_pending(&state.db, limit, q.cursor).await?;
    Ok(Json(paginate(rows, limit)))
}

// ─── Handlers — private list ───────────────────────────────────────────────────

async fn add_private(
    State(state): State<AppState>,
    claims: Claims,
    Json(body): Json<AddPrivateRequest>,
) -> Result<impl IntoResponse> {
    let (grader, cert_number) = normalize_cert(&body.grader, &body.cert_number)?;

    let report = db_upsert_report(
        &state.db,
        &grader,
        &cert_number,
        claims.user_id,
        Some(claims.workspace_id),
        None,
        body.notes.as_deref(),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(to_dto(report))))
}

async fn list_private(
    State(state): State<AppState>,
    claims: Claims,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse> {
    let limit = clamp_limit(q.limit);
    let rows: Vec<StolenReportRow> = sqlx::query_as(
        "SELECT id, grader, cert_number, reporter_user_id, workspace_id, \
                scope, status, evidence, reporter_notes, moderator_notes, \
                dispute_user_id, dispute_notes, dispute_at, \
                confirmed_at, resolved_at, created_at, updated_at \
         FROM stolen_reports \
         WHERE scope = 'private' AND workspace_id = $1 AND status != 'resolved' \
           AND ($2::uuid IS NULL OR id < $2) \
         ORDER BY created_at DESC \
         LIMIT $3",
    )
    .bind(claims.workspace_id)
    .bind(q.cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(paginate(rows, limit)))
}

async fn remove_private(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("private entry"))?;

    if row.scope != "private" || row.workspace_id != Some(claims.workspace_id) {
        return Err(ApiError::forbidden());
    }

    sqlx::query(
        "DELETE FROM stolen_reports WHERE id = $1 AND workspace_id = $2 AND scope = 'private'",
    )
    .bind(id)
    .bind(claims.workspace_id)
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn resolve_private(
    State(state): State<AppState>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let row = db_get_report(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("private entry"))?;

    if row.scope != "private" || row.workspace_id != Some(claims.workspace_id) {
        return Err(ApiError::forbidden());
    }

    if row.status == "resolved" {
        return Err(ApiError::validation("already resolved"));
    }

    let updated =
        db_transition_report(&state.db, id, "resolved", None, Some(Utc::now()), None).await?;

    Ok(Json(to_dto(updated)))
}

// ─── DB layer ─────────────────────────────────────────────────────────────────

/// Insert a new report or, for a duplicate cert (same scope + workspace), link the
/// reporter to the existing record and return it unchanged.
async fn db_upsert_report(
    pool: &PgPool,
    grader: &str,
    cert_number: &str,
    reporter_user_id: Uuid,
    workspace_id: Option<Uuid>,
    evidence: Option<&str>,
    notes: Option<&str>,
) -> Result<StolenReportRow> {
    let scope = if workspace_id.is_some() {
        "private"
    } else {
        "shared"
    };
    // Private entries are always trusted; shared entries start as pending.
    let initial_status = if workspace_id.is_some() {
        "confirmed"
    } else {
        "pending"
    };

    // Check for an existing non-rejected report for this cert in the same scope/workspace.
    let existing: Option<StolenReportRow> = sqlx::query_as(
        "SELECT id, grader, cert_number, reporter_user_id, workspace_id, \
                scope, status, evidence, reporter_notes, moderator_notes, \
                dispute_user_id, dispute_notes, dispute_at, \
                confirmed_at, resolved_at, created_at, updated_at \
         FROM stolen_reports \
         WHERE grader = $1 AND cert_number = $2 AND scope = $3 \
           AND (workspace_id = $4 OR ($4::uuid IS NULL AND workspace_id IS NULL)) \
           AND status != 'rejected' \
         LIMIT 1",
    )
    .bind(grader)
    .bind(cert_number)
    .bind(scope)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = existing {
        // Duplicate: record this reporter without changing the report.
        sqlx::query(
            "INSERT INTO stolen_report_reporters (report_id, user_id, reported_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(row.id)
        .bind(reporter_user_id)
        .execute(pool)
        .await?;

        return Ok(row);
    }

    // Create the report.
    let row: StolenReportRow = sqlx::query_as(
        "INSERT INTO stolen_reports \
           (grader, cert_number, reporter_user_id, workspace_id, \
            scope, status, evidence, reporter_notes, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now()) \
         RETURNING id, grader, cert_number, reporter_user_id, workspace_id, \
                   scope, status, evidence, reporter_notes, moderator_notes, \
                   dispute_user_id, dispute_notes, dispute_at, \
                   confirmed_at, resolved_at, created_at, updated_at",
    )
    .bind(grader)
    .bind(cert_number)
    .bind(reporter_user_id)
    .bind(workspace_id)
    .bind(scope)
    .bind(initial_status)
    .bind(evidence)
    .bind(notes)
    .fetch_one(pool)
    .await?;

    // Record the original reporter.
    sqlx::query(
        "INSERT INTO stolen_report_reporters (report_id, user_id, reported_at) \
         VALUES ($1, $2, now()) \
         ON CONFLICT DO NOTHING",
    )
    .bind(row.id)
    .bind(reporter_user_id)
    .execute(pool)
    .await?;

    Ok(row)
}

async fn db_get_report(pool: &PgPool, id: Uuid) -> Result<Option<StolenReportRow>> {
    Ok(sqlx::query_as(
        "SELECT id, grader, cert_number, reporter_user_id, workspace_id, \
                scope, status, evidence, reporter_notes, moderator_notes, \
                dispute_user_id, dispute_notes, dispute_at, \
                confirmed_at, resolved_at, created_at, updated_at \
         FROM stolen_reports WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

async fn db_is_reporter(pool: &PgPool, report_id: Uuid, user_id: Uuid) -> Result<bool> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT true FROM stolen_report_reporters \
         WHERE report_id = $1 AND user_id = $2 \
         LIMIT 1",
    )
    .bind(report_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

async fn db_list_by_reporter(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    cursor: Option<Uuid>,
) -> Result<Vec<StolenReportRow>> {
    Ok(sqlx::query_as(
        "SELECT sr.id, sr.grader, sr.cert_number, sr.reporter_user_id, sr.workspace_id, \
                sr.scope, sr.status, sr.evidence, sr.reporter_notes, sr.moderator_notes, \
                sr.dispute_user_id, sr.dispute_notes, sr.dispute_at, \
                sr.confirmed_at, sr.resolved_at, sr.created_at, sr.updated_at \
         FROM stolen_reports sr \
         JOIN stolen_report_reporters srr ON srr.report_id = sr.id \
         WHERE srr.user_id = $1 \
           AND ($2::uuid IS NULL OR sr.id < $2) \
         ORDER BY sr.created_at DESC \
         LIMIT $3",
    )
    .bind(user_id)
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

async fn db_list_pending(
    pool: &PgPool,
    limit: i64,
    cursor: Option<Uuid>,
) -> Result<Vec<StolenReportRow>> {
    Ok(sqlx::query_as(
        "SELECT id, grader, cert_number, reporter_user_id, workspace_id, \
                scope, status, evidence, reporter_notes, moderator_notes, \
                dispute_user_id, dispute_notes, dispute_at, \
                confirmed_at, resolved_at, created_at, updated_at \
         FROM stolen_reports \
         WHERE scope = 'shared' AND status IN ('pending', 'disputed') \
           AND ($1::uuid IS NULL OR id < $1) \
         ORDER BY created_at ASC \
         LIMIT $2",
    )
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

async fn db_transition_report(
    pool: &PgPool,
    id: Uuid,
    new_status: &str,
    confirmed_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
    notes: Option<&str>,
) -> Result<StolenReportRow> {
    Ok(sqlx::query_as(
        "UPDATE stolen_reports \
         SET status = $2, \
             moderator_notes = COALESCE($3, moderator_notes), \
             confirmed_at = COALESCE($4, confirmed_at), \
             resolved_at  = COALESCE($5, resolved_at), \
             updated_at   = now() \
         WHERE id = $1 \
         RETURNING id, grader, cert_number, reporter_user_id, workspace_id, \
                   scope, status, evidence, reporter_notes, moderator_notes, \
                   dispute_user_id, dispute_notes, dispute_at, \
                   confirmed_at, resolved_at, created_at, updated_at",
    )
    .bind(id)
    .bind(new_status)
    .bind(notes)
    .bind(confirmed_at)
    .bind(resolved_at)
    .fetch_one(pool)
    .await?)
}

async fn db_dispute_report(
    pool: &PgPool,
    id: Uuid,
    disputer_id: Uuid,
    notes: &str,
) -> Result<StolenReportRow> {
    Ok(sqlx::query_as(
        "UPDATE stolen_reports \
         SET status = 'disputed', \
             dispute_user_id = $2, \
             dispute_notes   = $3, \
             dispute_at      = now(), \
             updated_at      = now() \
         WHERE id = $1 \
         RETURNING id, grader, cert_number, reporter_user_id, workspace_id, \
                   scope, status, evidence, reporter_notes, moderator_notes, \
                   dispute_user_id, dispute_notes, dispute_at, \
                   confirmed_at, resolved_at, created_at, updated_at",
    )
    .bind(id)
    .bind(disputer_id)
    .bind(notes)
    .fetch_one(pool)
    .await?)
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn normalize_cert(grader: &str, cert_number: &str) -> Result<(String, String)> {
    let g = grader.trim().to_uppercase();
    let c = cert_number.trim().to_string();
    if g.is_empty() || c.is_empty() {
        return Err(ApiError::validation("grader and cert_number are required"));
    }
    Ok((g, c))
}

fn clamp_limit(raw: Option<i64>) -> i64 {
    raw.unwrap_or(25).clamp(1, 200)
}

fn paginate(rows: Vec<StolenReportRow>, limit: i64) -> ListResponse<StolenReportDto> {
    let next_cursor = if rows.len() as i64 == limit {
        rows.last().map(|r| r.id)
    } else {
        None
    };
    ListResponse {
        data: rows.into_iter().map(to_dto).collect(),
        next_cursor,
    }
}

fn to_dto(row: StolenReportRow) -> StolenReportDto {
    StolenReportDto {
        id: row.id,
        grader: row.grader,
        cert_number: row.cert_number,
        scope: row.scope,
        status: row.status,
        evidence: row.evidence,
        notes: row.reporter_notes,
        moderator_notes: row.moderator_notes,
        dispute_notes: row.dispute_notes,
        dispute_at: row.dispute_at,
        confirmed_at: row.confirmed_at,
        resolved_at: row.resolved_at,
        created_at: row.created_at,
    }
}

fn require_moderator(claims: &Claims) -> Result<()> {
    if !claims.is_platform_moderator {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

async fn write_audit_log(
    pool: &PgPool,
    actor_id: Uuid,
    action: &str,
    entity_type: &str,
    entity_id: Uuid,
    meta: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, entity_type, entity_id, at, meta) \
         VALUES ($1, $2, $3, $4, now(), $5)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(meta)
    .execute(pool)
    .await?;
    Ok(())
}

/// Enqueue an apalis job to re-scan all CardInstances after a new confirmed entry lands.
async fn enqueue_rescan(pool: &PgPool, triggered_by: Uuid) -> Result<()> {
    let payload = serde_json::to_value(StolenRescanJob { triggered_by_report_id: triggered_by })
        .expect("serializable");
    sqlx::query(
        "INSERT INTO apalis.jobs (id, job, status, max_attempts, run_at, job_type, priority) \
         VALUES (gen_random_uuid(), $1, 'Pending', 3, now(), 'stolen_rescan', 5)",
    )
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

// ─── Public API for catalogue and grading streams ─────────────────────────────

/// Check whether a graded card's cert is on the confirmed stolen list (shared or
/// this workspace's private list) and, if so, raise a `RiskFlag(kind=stolen)`.
///
/// Returns `true` when a new flag was created.  Returns `false` when no match was
/// found.  Re-entrant: calling it again for the same target is a no-op.
///
/// # Callers
/// - `crate::catalogue` — immediately after a card is catalogued
/// - `crate::grading`  — after a cert is successfully verified
pub async fn check_and_flag_stolen(
    pool: &PgPool,
    grader: &str,
    cert_number: &str,
    workspace_id: Uuid,
    target_id: Uuid,
    /// `"instance"` for a graded CardInstance; `"inventory"` for raw InventoryItem
    target_type: &str,
) -> Result<bool> {
    let grader = grader.trim().to_uppercase();
    let cert_number = cert_number.trim();

    // Raw cards have no cert; they are not reliably checkable in v1.
    if cert_number.is_empty() {
        return Ok(false);
    }

    // Match against the shared confirmed list OR this workspace's private list.
    let hit: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, scope \
         FROM stolen_reports \
         WHERE grader = $1 AND cert_number = $2 \
           AND status = 'confirmed' \
           AND (scope = 'shared' OR (scope = 'private' AND workspace_id = $3)) \
         LIMIT 1",
    )
    .bind(&grader)
    .bind(cert_number)
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    let Some((report_id, list_scope)) = hit else {
        return Ok(false);
    };

    // Avoid duplicate open flags for the same target.
    let already_flagged: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM risk_flags \
         WHERE workspace_id = $1 AND kind = 'stolen' \
           AND target_type = $2 AND target_id = $3 \
           AND status != 'dismissed' \
         LIMIT 1",
    )
    .bind(workspace_id)
    .bind(target_type)
    .bind(target_id)
    .fetch_optional(pool)
    .await?;

    if already_flagged.is_some() {
        return Ok(true); // flag already exists; no-op
    }

    let detail = serde_json::json!({
        "kind": "stolen_cert_match",
        "stolen_report_id": report_id,
        "cert_number": cert_number,
        "grader": grader,
        "list_scope": list_scope,
    });

    sqlx::query(
        "INSERT INTO risk_flags \
           (id, workspace_id, kind, severity, target_type, target_id, status, detail, created_at) \
         VALUES (gen_random_uuid(), $1, 'stolen', 'high', $2, $3, 'open', $4, now())",
    )
    .bind(workspace_id)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(pool)
    .await?;

    Ok(true)
}

/// Re-scan every CardInstance in `workspace_id` against the current confirmed stolen list.
///
/// Called by [`run_stolen_rescan`] and can be invoked directly in tests.
/// Returns the number of new flags raised.
pub async fn rescan_workspace_stolen(pool: &PgPool, workspace_id: Uuid) -> Result<usize> {
    let instances: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, grader, cert_number FROM card_instances WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await?;

    let mut flagged = 0usize;
    for (id, grader, cert) in instances {
        match check_and_flag_stolen(pool, &grader, &cert, workspace_id, id, "instance").await {
            Ok(true) => flagged += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    instance_id = %id,
                    error = %e,
                    "Error during stolen-cert re-scan of instance"
                );
            }
        }
    }
    Ok(flagged)
}

// ─── Apalis job ───────────────────────────────────────────────────────────────

/// Payload for the background re-scan job.
///
/// Register this with apalis in `crates/api/src/jobs/mod.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StolenRescanJob {
    pub triggered_by_report_id: Uuid,
}

/// apalis job handler — re-scans every workspace when a new confirmed entry lands.
///
/// Wire this into the job runner in `crates/api/src/jobs/mod.rs`:
/// ```ignore
/// WorkerBuilder::new("stolen_rescan")
///     .data(pool.clone())
///     .build_fn(stolen::run_stolen_rescan)
/// ```
pub async fn run_stolen_rescan(
    job: apalis::prelude::Data<StolenRescanJob>,
    pool: apalis::prelude::Data<PgPool>,
) -> std::result::Result<(), apalis::prelude::JobError> {
    let pool = pool.into_inner();
    let triggered_by = job.into_inner().triggered_by_report_id;

    let workspace_ids: Vec<(Uuid,)> =
        sqlx::query_as("SELECT id FROM workspaces")
            .fetch_all(pool.as_ref())
            .await
            .map_err(|e| apalis::prelude::JobError::Failed(Box::new(e)))?;

    let mut total_flagged = 0usize;
    for (ws_id,) in workspace_ids {
        match rescan_workspace_stolen(pool.as_ref(), ws_id).await {
            Ok(n) => total_flagged += n,
            Err(e) => {
                tracing::warn!(
                    workspace_id = %ws_id,
                    error = %e,
                    "Stolen-cert rescan failed for workspace"
                );
            }
        }
    }

    tracing::info!(
        triggered_by = %triggered_by,
        flagged = total_flagged,
        "Stolen cert rescan complete"
    );

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests for pure logic; integration tests use testcontainers (see
    // crates/api/tests/) and require a running Postgres via the test harness.

    #[test]
    fn normalize_cert_uppercase() {
        let (g, c) = normalize_cert("psa", "12345678").unwrap();
        assert_eq!(g, "PSA");
        assert_eq!(c, "12345678");
    }

    #[test]
    fn normalize_cert_trims_whitespace() {
        let (g, c) = normalize_cert("  Cgc  ", " 99999 ").unwrap();
        assert_eq!(g, "CGC");
        assert_eq!(c, "99999");
    }

    #[test]
    fn normalize_cert_empty_grader_is_error() {
        assert!(normalize_cert("", "123").is_err());
    }

    #[test]
    fn normalize_cert_empty_number_is_error() {
        assert!(normalize_cert("PSA", "").is_err());
    }

    #[test]
    fn clamp_limit_defaults_to_25() {
        assert_eq!(clamp_limit(None), 25);
    }

    #[test]
    fn clamp_limit_enforces_max() {
        assert_eq!(clamp_limit(Some(9999)), 200);
    }

    #[test]
    fn clamp_limit_enforces_min() {
        assert_eq!(clamp_limit(Some(0)), 1);
    }

    #[test]
    fn to_dto_maps_all_fields() {
        let now = Utc::now();
        let row = StolenReportRow {
            id: Uuid::new_v4(),
            grader: "PSA".into(),
            cert_number: "12345678".into(),
            reporter_user_id: Uuid::new_v4(),
            workspace_id: None,
            scope: "shared".into(),
            status: "pending".into(),
            evidence: Some("photo proof".into()),
            reporter_notes: Some("my notes".into()),
            moderator_notes: None,
            dispute_user_id: None,
            dispute_notes: None,
            dispute_at: None,
            confirmed_at: None,
            resolved_at: None,
            created_at: now,
            updated_at: now,
        };
        let dto = to_dto(row);
        assert_eq!(dto.grader, "PSA");
        assert_eq!(dto.status, "pending");
        assert_eq!(dto.notes.as_deref(), Some("my notes"));
    }
}
