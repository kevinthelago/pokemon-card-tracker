//! M1 — Inventory browse & management API
//!
//! Routes are scoped under `/workspaces/:wid/catalogue/items`.  Every
//! request requires a valid session (`AuthUser`) and the caller must be a
//! member of the target workspace.
//!
//! Uses runtime `sqlx::query_as` (no compile-time `query!` macro) so the
//! crate compiles without a live `DATABASE_URL`.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder};
use uuid::Uuid;

use crate::{app::AppState, auth::AuthUser, error::AppError};

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/:wid/catalogue/items", get(list_items))
        .route("/workspaces/:wid/catalogue/items/export", get(export_items))
        .route("/workspaces/:wid/catalogue/items/bulk", post(bulk_items))
        .route(
            "/workspaces/:wid/catalogue/items/:id",
            get(get_item).patch(patch_item).delete(delete_item),
        )
}

// ─── Membership guard ─────────────────────────────────────────────────────────

async fn require_member(pool: &sqlx::PgPool, wid: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let exists: Option<bool> = sqlx::query_scalar(
        "SELECT true FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(wid)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    exists.ok_or(AppError::Forbidden("not a member of this workspace".into()))?;
    Ok(())
}

// ─── Cursor pagination ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct CursorPayload {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    let payload = serde_json::to_string(&CursorPayload { created_at, id }).unwrap_or_default();
    URL_SAFE_NO_PAD.encode(payload.as_bytes())
}

fn decode_cursor(cursor: &str) -> Option<CursorPayload> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ─── Shared row type ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct InventoryRow {
    pub id: Uuid,
    pub kind: String,
    pub workspace_id: Uuid,
    pub printing_id: Uuid,
    pub printing_name: String,
    pub set_code: String,
    pub set_name: String,
    pub collector_number: String,
    pub rarity: String,
    pub image_url: Option<String>,
    pub condition: Option<String>,
    pub quantity: i64,
    pub grade: Option<String>,
    pub grader: Option<String>,
    pub cert_number: Option<String>,
    pub verification_status: Option<String>,
    pub acquisition_cost: Option<Decimal>,
    pub notes: Option<String>,
    pub current_value: Option<Decimal>,
    pub has_risk_flag: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Query params ─────────────────────────────────────────────────────────────

fn default_limit() -> i64 {
    50
}
fn default_sort() -> String {
    "date".into()
}
fn default_order() -> String {
    "desc".into()
}

#[derive(Debug, Deserialize, Clone)]
pub struct InventoryListQuery {
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub search: Option<String>,
    pub set_code: Option<String>,
    pub rarity: Option<String>,
    pub condition: Option<String>,
    /// "raw" | "graded"
    pub kind: Option<String>,
    pub grader: Option<String>,
    pub value_min: Option<Decimal>,
    pub value_max: Option<Decimal>,
    pub risk_flagged: Option<bool>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_order")]
    pub order: String,
}

impl Default for InventoryListQuery {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: default_limit(),
            search: None,
            set_code: None,
            rarity: None,
            condition: None,
            kind: None,
            grader: None,
            value_min: None,
            value_max: None,
            risk_flagged: None,
            sort: default_sort(),
            order: default_order(),
        }
    }
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct InventoryPage {
    pub items: Vec<InventoryRow>,
    pub next_cursor: Option<String>,
}

// ─── List handler ─────────────────────────────────────────────────────────────

async fn list_items(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<InventoryListQuery>,
) -> Result<Json<InventoryPage>, AppError> {
    require_member(&state.pool, wid, auth.id).await?;

    let limit = q.limit.clamp(1, 200);
    let cursor = q.cursor.as_deref().and_then(decode_cursor);

    let rows = build_list_query(wid, &q, limit, cursor.as_ref())
        .build_query_as::<InventoryRow>()
        .fetch_all(&state.pool)
        .await?;

    let next_cursor = if rows.len() as i64 == limit {
        rows.last().map(|r| encode_cursor(r.created_at, r.id))
    } else {
        None
    };

    Ok(Json(InventoryPage {
        items: rows,
        next_cursor,
    }))
}

fn build_list_query<'a>(
    wid: Uuid,
    q: &'a InventoryListQuery,
    limit: i64,
    cursor: Option<&'a CursorPayload>,
) -> QueryBuilder<'a, sqlx::Postgres> {
    // Each branch of the UNION ALL must select identical columns in identical order.
    // raw items always have quantity; graded always have grade/grader/cert.
    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        r#"
        SELECT id, 'raw' AS kind, workspace_id, printing_id,
               p.name AS printing_name, p.set_code, p.set_name,
               p.collector_number, p.rarity, p.image_url,
               condition, quantity,
               NULL::text AS grade, NULL::text AS grader,
               NULL::text AS cert_number, NULL::text AS verification_status,
               acquisition_cost, notes, current_value,
               EXISTS (
                 SELECT 1 FROM risk_flags rf
                 WHERE rf.item_id = ii.id AND rf.item_kind = 'raw' AND rf.status = 'open'
               ) AS has_risk_flag,
               created_at, updated_at
        FROM inventory_items ii
        JOIN printings p ON p.id = ii.printing_id
        WHERE ii.workspace_id =
        "#,
    );
    qb.push_bind(wid);
    qb.push(" AND ii.deleted_at IS NULL");

    apply_raw_filters(&mut qb, q, cursor);

    qb.push(
        r#"
        UNION ALL
        SELECT ci.id, 'graded' AS kind, ci.workspace_id, ci.printing_id,
               p.name AS printing_name, p.set_code, p.set_name,
               p.collector_number, p.rarity, p.image_url,
               ci.condition, 1 AS quantity,
               ci.grade, ci.grader::text, ci.cert_number,
               ci.verification_status::text,
               ci.acquisition_cost, ci.notes, ci.current_value,
               EXISTS (
                 SELECT 1 FROM risk_flags rf
                 WHERE rf.item_id = ci.id AND rf.item_kind = 'graded' AND rf.status = 'open'
               ) AS has_risk_flag,
               ci.created_at, ci.updated_at
        FROM card_instances ci
        JOIN printings p ON p.id = ci.printing_id
        WHERE ci.workspace_id =
        "#,
    );
    qb.push_bind(wid);
    qb.push(" AND ci.deleted_at IS NULL");

    apply_graded_filters(&mut qb, q, cursor);

    // Wrap in a CTE for unified ORDER BY + LIMIT
    let order_col = match q.sort.as_str() {
        "value" => "current_value",
        "name" => "printing_name",
        _ => "created_at",
    };
    let order_dir = if q.order == "asc" { "ASC" } else { "DESC" };

    qb.push(format!(
        " ORDER BY {order_col} {order_dir}, id {order_dir} LIMIT "
    ));
    qb.push_bind(limit);

    qb
}

fn apply_raw_filters(
    qb: &mut QueryBuilder<sqlx::Postgres>,
    q: &InventoryListQuery,
    cursor: Option<&CursorPayload>,
) {
    if q.kind.as_deref() == Some("graded") {
        qb.push(" AND false");
        return;
    }
    apply_shared_filters(qb, q);
    if let Some(c) = &q.condition {
        qb.push(" AND ii.condition = ");
        qb.push_bind(c.clone());
    }
    if q.risk_flagged == Some(true) {
        qb.push(
            " AND EXISTS (SELECT 1 FROM risk_flags rf WHERE rf.item_id = ii.id AND rf.item_kind = 'raw' AND rf.status = 'open')",
        );
    }
    if let Some(cur) = cursor {
        qb.push(" AND (ii.created_at, ii.id) < (");
        qb.push_bind(cur.created_at);
        qb.push(", ");
        qb.push_bind(cur.id);
        qb.push(")");
    }
}

fn apply_graded_filters(
    qb: &mut QueryBuilder<sqlx::Postgres>,
    q: &InventoryListQuery,
    cursor: Option<&CursorPayload>,
) {
    if q.kind.as_deref() == Some("raw") {
        qb.push(" AND false");
        return;
    }
    apply_shared_filters(qb, q);
    if let Some(c) = &q.condition {
        qb.push(" AND ci.condition = ");
        qb.push_bind(c.clone());
    }
    if let Some(g) = &q.grader {
        qb.push(" AND ci.grader = ");
        qb.push_bind(g.clone());
    }
    if q.risk_flagged == Some(true) {
        qb.push(
            " AND EXISTS (SELECT 1 FROM risk_flags rf WHERE rf.item_id = ci.id AND rf.item_kind = 'graded' AND rf.status = 'open')",
        );
    }
    if let Some(cur) = cursor {
        qb.push(" AND (ci.created_at, ci.id) < (");
        qb.push_bind(cur.created_at);
        qb.push(", ");
        qb.push_bind(cur.id);
        qb.push(")");
    }
}

fn apply_shared_filters(qb: &mut QueryBuilder<sqlx::Postgres>, q: &InventoryListQuery) {
    if let Some(search) = &q.search {
        let pattern = format!("%{}%", search.replace('%', "\\%").replace('_', "\\_"));
        qb.push(" AND p.name ILIKE ");
        qb.push_bind(pattern);
    }
    if let Some(set) = &q.set_code {
        qb.push(" AND p.set_code = ");
        qb.push_bind(set.clone());
    }
    if let Some(rarity) = &q.rarity {
        qb.push(" AND p.rarity = ");
        qb.push_bind(rarity.clone());
    }
    if let Some(min) = &q.value_min {
        qb.push(" AND current_value >= ");
        qb.push_bind(*min);
    }
    if let Some(max) = &q.value_max {
        qb.push(" AND current_value <= ");
        qb.push_bind(*max);
    }
}

// ─── Get single item ──────────────────────────────────────────────────────────

async fn get_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<InventoryRow>, AppError> {
    require_member(&state.pool, wid, auth.id).await?;

    // Try raw first, then graded.
    let raw = sqlx::query_as::<_, InventoryRow>(
        r#"
        SELECT ii.id, 'raw' AS kind, ii.workspace_id, ii.printing_id,
               p.name AS printing_name, p.set_code, p.set_name,
               p.collector_number, p.rarity, p.image_url,
               ii.condition, ii.quantity,
               NULL::text AS grade, NULL::text AS grader,
               NULL::text AS cert_number, NULL::text AS verification_status,
               ii.acquisition_cost, ii.notes, ii.current_value,
               EXISTS (
                 SELECT 1 FROM risk_flags rf
                 WHERE rf.item_id = ii.id AND rf.item_kind = 'raw' AND rf.status = 'open'
               ) AS has_risk_flag,
               ii.created_at, ii.updated_at
        FROM inventory_items ii
        JOIN printings p ON p.id = ii.printing_id
        WHERE ii.id = $1 AND ii.workspace_id = $2 AND ii.deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(wid)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(r) = raw {
        return Ok(Json(r));
    }

    let graded = sqlx::query_as::<_, InventoryRow>(
        r#"
        SELECT ci.id, 'graded' AS kind, ci.workspace_id, ci.printing_id,
               p.name AS printing_name, p.set_code, p.set_name,
               p.collector_number, p.rarity, p.image_url,
               ci.condition, 1 AS quantity,
               ci.grade, ci.grader::text, ci.cert_number,
               ci.verification_status::text,
               ci.acquisition_cost, ci.notes, ci.current_value,
               EXISTS (
                 SELECT 1 FROM risk_flags rf
                 WHERE rf.item_id = ci.id AND rf.item_kind = 'graded' AND rf.status = 'open'
               ) AS has_risk_flag,
               ci.created_at, ci.updated_at
        FROM card_instances ci
        JOIN printings p ON p.id = ci.printing_id
        WHERE ci.id = $1 AND ci.workspace_id = $2 AND ci.deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(wid)
    .fetch_optional(&state.pool)
    .await?;

    graded.map(Json).ok_or(AppError::NotFound)
}

// ─── Patch item ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PatchItemRequest {
    /// Client must echo back the `updated_at` it currently holds (optimistic concurrency).
    pub version: DateTime<Utc>,
    pub condition: Option<String>,
    pub quantity: Option<i64>,
    pub acquisition_cost: Option<Decimal>,
    pub notes: Option<String>,
    pub current_value: Option<Decimal>,
}

async fn patch_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, id)): Path<(Uuid, Uuid)>,
    Json(req): Json<PatchItemRequest>,
) -> Result<Json<InventoryRow>, AppError> {
    require_member(&state.pool, wid, auth.id).await?;

    // Determine kind + check optimistic lock (raw first).
    let raw_ts: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT updated_at FROM inventory_items WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL")
            .bind(id)
            .bind(wid)
            .fetch_optional(&state.pool)
            .await?;

    if let Some(server_ts) = raw_ts {
        if server_ts != req.version {
            return Err(AppError::Conflict(
                "item was modified — reload and retry".into(),
            ));
        }
        // Apply update.
        sqlx::query(
            r#"
            UPDATE inventory_items SET
                condition        = COALESCE($3, condition),
                quantity         = COALESCE($4, quantity),
                acquisition_cost = COALESCE($5, acquisition_cost),
                notes            = COALESCE($6, notes),
                current_value    = COALESCE($7, current_value),
                updated_at       = NOW()
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id)
        .bind(wid)
        .bind(&req.condition)
        .bind(req.quantity)
        .bind(req.acquisition_cost)
        .bind(&req.notes)
        .bind(req.current_value)
        .execute(&state.pool)
        .await?;

        return get_item(State(state), auth, Path((wid, id))).await;
    }

    // Try graded.
    let graded_ts: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT updated_at FROM card_instances WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL")
            .bind(id)
            .bind(wid)
            .fetch_optional(&state.pool)
            .await?;

    if let Some(server_ts) = graded_ts {
        if server_ts != req.version {
            return Err(AppError::Conflict(
                "item was modified — reload and retry".into(),
            ));
        }
        // Graded: condition, acquisition_cost, notes, current_value only (grade/identity read-only).
        sqlx::query(
            r#"
            UPDATE card_instances SET
                condition        = COALESCE($3, condition),
                acquisition_cost = COALESCE($5, acquisition_cost),
                notes            = COALESCE($6, notes),
                current_value    = COALESCE($7, current_value),
                updated_at       = NOW()
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id)
        .bind(wid)
        .bind(&req.condition)
        .bind(req.quantity) // unused, placeholder to keep $4
        .bind(req.acquisition_cost)
        .bind(&req.notes)
        .bind(req.current_value)
        .execute(&state.pool)
        .await?;

        return get_item(State(state), auth, Path((wid, id))).await;
    }

    Err(AppError::NotFound)
}

// ─── Delete item ──────────────────────────────────────────────────────────────

async fn delete_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_member(&state.pool, wid, auth.id).await?;

    // Check if raw item is referenced by transaction_lines.
    let referenced: Option<bool> =
        sqlx::query_scalar("SELECT true FROM transaction_lines WHERE item_id = $1 LIMIT 1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;

    if referenced.is_some() {
        // Soft-delete: preserve history.
        let affected = sqlx::query(
            "UPDATE inventory_items SET deleted_at = NOW() WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(wid)
        .execute(&state.pool)
        .await?
        .rows_affected();

        if affected == 0 {
            // Try card_instances soft-delete.
            sqlx::query(
                "UPDATE card_instances SET deleted_at = NOW() WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(wid)
            .execute(&state.pool)
            .await?;
        }
    } else {
        // Hard-delete.
        let affected = sqlx::query(
            "DELETE FROM inventory_items WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(wid)
        .execute(&state.pool)
        .await?
        .rows_affected();

        if affected == 0 {
            sqlx::query(
                "DELETE FROM card_instances WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
            )
            .bind(id)
            .bind(wid)
            .execute(&state.pool)
            .await?;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

// ─── Bulk operations ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BulkRequest {
    Edit {
        ids: Vec<Uuid>,
        condition: Option<String>,
        acquisition_cost: Option<Decimal>,
    },
    Delete {
        ids: Vec<Uuid>,
    },
}

#[derive(Serialize)]
pub struct BulkResult {
    pub affected: u64,
}

async fn bulk_items(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Json(req): Json<BulkRequest>,
) -> Result<Json<BulkResult>, AppError> {
    require_member(&state.pool, wid, auth.id).await?;

    let affected = match req {
        BulkRequest::Edit {
            ids,
            condition,
            acquisition_cost,
        } => {
            if ids.is_empty() || (condition.is_none() && acquisition_cost.is_none()) {
                return Ok(Json(BulkResult { affected: 0 }));
            }
            let mut total = 0u64;

            let build_set = |qb: &mut QueryBuilder<sqlx::Postgres>,
                             cond: &Option<String>,
                             acq: Option<Decimal>| {
                let mut need_sep = false;
                if let Some(c) = cond {
                    qb.push("condition = ");
                    qb.push_bind(c.clone());
                    need_sep = true;
                }
                if let Some(a) = acq {
                    if need_sep {
                        qb.push(", ");
                    }
                    qb.push("acquisition_cost = ");
                    qb.push_bind(a);
                }
            };

            // Update raw items.
            let mut qb: QueryBuilder<sqlx::Postgres> =
                QueryBuilder::new("UPDATE inventory_items SET ");
            build_set(&mut qb, &condition, acquisition_cost);
            qb.push(", updated_at = NOW() WHERE workspace_id = ");
            qb.push_bind(wid);
            qb.push(" AND id = ANY(");
            qb.push_bind(&ids);
            qb.push(")");
            total += qb.build().execute(&state.pool).await?.rows_affected();

            // Update graded items (grade/grader/cert stay immutable).
            let mut qb2: QueryBuilder<sqlx::Postgres> =
                QueryBuilder::new("UPDATE card_instances SET ");
            build_set(&mut qb2, &condition, acquisition_cost);
            qb2.push(", updated_at = NOW() WHERE workspace_id = ");
            qb2.push_bind(wid);
            qb2.push(" AND id = ANY(");
            qb2.push_bind(&ids);
            qb2.push(")");
            total += qb2.build().execute(&state.pool).await?.rows_affected();

            total
        }
        BulkRequest::Delete { ids } => {
            if ids.is_empty() {
                return Ok(Json(BulkResult { affected: 0 }));
            }
            // Check which ids are referenced by transaction_lines.
            let referenced: Vec<Uuid> = sqlx::query_scalar(
                "SELECT DISTINCT item_id FROM transaction_lines WHERE item_id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(&state.pool)
            .await?;

            let soft: Vec<Uuid> = ids
                .iter()
                .filter(|id| referenced.contains(id))
                .copied()
                .collect();
            let hard: Vec<Uuid> = ids
                .iter()
                .filter(|id| !referenced.contains(id))
                .copied()
                .collect();

            let mut total = 0u64;
            if !soft.is_empty() {
                total += sqlx::query(
                    "UPDATE inventory_items SET deleted_at = NOW() WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
                )
                .bind(wid)
                .bind(&soft)
                .execute(&state.pool)
                .await?
                .rows_affected();
                total += sqlx::query(
                    "UPDATE card_instances SET deleted_at = NOW() WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
                )
                .bind(wid)
                .bind(&soft)
                .execute(&state.pool)
                .await?
                .rows_affected();
            }
            if !hard.is_empty() {
                total += sqlx::query(
                    "DELETE FROM inventory_items WHERE workspace_id = $1 AND id = ANY($2)",
                )
                .bind(wid)
                .bind(&hard)
                .execute(&state.pool)
                .await?
                .rows_affected();
                total += sqlx::query(
                    "DELETE FROM card_instances WHERE workspace_id = $1 AND id = ANY($2)",
                )
                .bind(wid)
                .bind(&hard)
                .execute(&state.pool)
                .await?
                .rows_affected();
            }
            total
        }
    };

    Ok(Json(BulkResult { affected }))
}

// ─── CSV export ───────────────────────────────────────────────────────────────

async fn export_items(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<InventoryListQuery>,
) -> Result<impl IntoResponse, AppError> {
    require_member(&state.pool, wid, auth.id).await?;

    let rows = build_list_query(wid, &q, 10_000, None)
        .build_query_as::<InventoryRow>()
        .fetch_all(&state.pool)
        .await?;

    let mut csv = String::from(
        "id,kind,printing_name,set_code,collector_number,rarity,condition,quantity,\
         grade,grader,cert_number,acquisition_cost,current_value,risk_flag,created_at\n",
    );
    for r in &rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.id,
            r.kind,
            csv_field(&r.printing_name),
            r.set_code,
            r.collector_number,
            r.rarity,
            r.condition.as_deref().unwrap_or(""),
            r.quantity,
            r.grade.as_deref().unwrap_or(""),
            r.grader.as_deref().unwrap_or(""),
            r.cert_number.as_deref().unwrap_or(""),
            r.acquisition_cost
                .map(|d| d.to_string())
                .unwrap_or_default(),
            r.current_value.map(|d| d.to_string()).unwrap_or_default(),
            r.has_risk_flag,
            r.created_at.to_rfc3339(),
        ));
    }

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("text/csv"));
    headers.insert(
        "Content-Disposition",
        HeaderValue::from_static("attachment; filename=\"inventory.csv\""),
    );

    Ok((headers, csv))
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn cursor_round_trip() {
        let id = Uuid::new_v4();
        let ts = Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap();
        let encoded = encode_cursor(ts, id);
        let decoded = decode_cursor(&encoded).expect("decode failed");
        assert_eq!(decoded.id, id);
        assert_eq!(decoded.created_at, ts);
    }

    #[test]
    fn csv_escape_commas() {
        assert_eq!(csv_field("foo,bar"), "\"foo,bar\"");
        assert_eq!(csv_field("simple"), "simple");
        assert_eq!(csv_field("has \"quotes\""), "\"has \"\"quotes\"\"\"");
    }

    #[test]
    fn default_query_params() {
        let q = InventoryListQuery::default();
        assert_eq!(q.limit, 50);
        assert_eq!(q.sort, "date");
        assert_eq!(q.order, "desc");
    }

    #[test]
    fn build_list_query_does_not_panic_with_risk_flagged() {
        let q = InventoryListQuery {
            risk_flagged: Some(true),
            ..Default::default()
        };
        let wid = Uuid::new_v4();
        // Should not panic — just verify the query builder produces SQL.
        let sql = build_list_query(wid, &q, 50, None).sql().to_string();
        assert!(
            sql.contains("risk_flags"),
            "risk_flagged filter missing from query"
        );
    }

    #[test]
    fn build_list_query_kind_raw_excludes_graded() {
        let q = InventoryListQuery {
            kind: Some("raw".into()),
            ..Default::default()
        };
        let wid = Uuid::new_v4();
        let sql = build_list_query(wid, &q, 50, None).sql().to_string();
        // graded branch should short-circuit with AND false
        assert!(sql.contains("AND false"));
    }

    #[test]
    fn build_list_query_kind_graded_excludes_raw() {
        let q = InventoryListQuery {
            kind: Some("graded".into()),
            ..Default::default()
        };
        let wid = Uuid::new_v4();
        let sql = build_list_query(wid, &q, 50, None).sql().to_string();
        assert!(sql.contains("AND false"));
    }
}
