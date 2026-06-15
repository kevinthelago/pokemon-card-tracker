//! Axum route handlers for the valuation feature.
//!
//! Mounts under `/api` via `app::create_router`. The `routes()` fn is the only
//! public symbol; `app.rs` nests it at `/api`.
//!
//! Workspace scoping: reads the `X-Workspace-Id` header (or `?workspace_id` query param)
//! since the stub auth middleware exposes AuthUser but not a workspace claim yet.
//!
//! Endpoints:
//!   GET  /valuations          — workspace valuation list + total
//!   POST /valuations/refresh  — on-demand price refresh (all or single printing)
//!   GET  /valuations/history  — price history snapshots for one printing

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::AuthUser;
use crate::error::AppError;

// ── Router ─────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/valuations", get(get_valuations))
        .route("/valuations/refresh", post(trigger_refresh))
        .route("/valuations/history", get(get_history))
}

// ── Workspace extraction ───────────────────────────────────────────────────

/// Extract workspace_id from `X-Workspace-Id` header or `workspace_id` query param.
fn workspace_from_headers_or_query(
    headers: &HeaderMap,
    workspace_id_param: Option<Uuid>,
) -> Result<Uuid, AppError> {
    if let Some(val) = headers.get("X-Workspace-Id") {
        let s = val.to_str().map_err(|_| AppError::BadRequest("invalid X-Workspace-Id header".into()))?;
        return Uuid::parse_str(s).map_err(|_| AppError::BadRequest("X-Workspace-Id is not a valid UUID".into()));
    }
    workspace_id_param.ok_or_else(|| AppError::BadRequest("workspace_id is required".into()))
}

// ── Response shapes ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ValuationItem {
    pub printing_id: String,
    pub card_name: String,
    pub set_code: String,
    pub condition: String,
    pub quantity: i32,
    pub value: ItemValue,
    pub line_total_usd: Option<Decimal>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ItemValue {
    Current {
        price_usd: Decimal,
        source: String,
        fetched_at: DateTime<Utc>,
    },
    Stale {
        price_usd: Decimal,
        source: String,
        fetched_at: DateTime<Utc>,
        stale_seconds: i64,
    },
    NoData,
    Pending,
}

#[derive(Debug, Serialize)]
pub struct GetValuationsResponse {
    pub data: Vec<ValuationItem>,
    pub total_usd: Decimal,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub refreshed: u32,
}

#[derive(Debug, Serialize)]
pub struct HistoryPoint {
    pub price_usd: Decimal,
    pub source: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub printing_id: String,
    pub data: Vec<HistoryPoint>,
}

// ── Query param structs ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkspaceQuery {
    pub workspace_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshParams {
    pub workspace_id: Option<Uuid>,
    pub printing_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryParams {
    pub workspace_id: Option<Uuid>,
    pub printing_id: String,
    pub limit: Option<i64>,
}

// ── Handlers ───────────────────────────────────────────────────────────────

async fn get_valuations(
    State(state): State<AppState>,
    _auth: AuthUser,
    headers: HeaderMap,
    Query(q): Query<WorkspaceQuery>,
) -> Result<Json<GetValuationsResponse>, AppError> {
    let workspace_id = workspace_from_headers_or_query(&headers, q.workspace_id)?;
    let svc = state.valuation_service();

    let rows = svc.workspace_valuations(workspace_id).await?;
    let total = svc.workspace_total(workspace_id).await?;

    let data = rows
        .into_iter()
        .map(|r| {
            let (value, line_total) = match (r.price, r.is_stale, r.fetched_at) {
                (Some(price), false, Some(fetched_at)) => (
                    ItemValue::Current {
                        price_usd: price,
                        source: r.source.unwrap_or_default(),
                        fetched_at,
                    },
                    Some(price * Decimal::from(r.quantity)),
                ),
                (Some(price), true, Some(fetched_at)) => {
                    let stale_seconds = (Utc::now() - fetched_at).num_seconds();
                    (
                        ItemValue::Stale {
                            price_usd: price,
                            source: r.source.unwrap_or_default(),
                            fetched_at,
                            stale_seconds,
                        },
                        Some(price * Decimal::from(r.quantity)),
                    )
                }
                (None, _, None) => (ItemValue::Pending, None),
                _ => (ItemValue::NoData, None),
            };
            ValuationItem {
                printing_id: r.printing_id,
                card_name: r.card_name,
                set_code: r.set_code,
                condition: r.condition,
                quantity: r.quantity,
                value,
                line_total_usd: line_total,
            }
        })
        .collect();

    Ok(Json(GetValuationsResponse { data, total_usd: total }))
}

async fn trigger_refresh(
    State(state): State<AppState>,
    _auth: AuthUser,
    headers: HeaderMap,
    Query(params): Query<RefreshParams>,
) -> Result<Json<RefreshResponse>, AppError> {
    // Workspace auth check (ensure caller belongs to the workspace) is deferred to
    // the full auth middleware; for now, require valid session via AuthUser.
    let _workspace_id = workspace_from_headers_or_query(&headers, params.workspace_id)?;
    let svc = state.valuation_service();

    let refreshed = if let Some(printing_id) = params.printing_id {
        svc.refresh(&printing_id).await?;
        1
    } else {
        svc.refresh_all().await?
    };

    Ok(Json(RefreshResponse { refreshed }))
}

async fn get_history(
    State(state): State<AppState>,
    _auth: AuthUser,
    headers: HeaderMap,
    Query(params): Query<HistoryParams>,
) -> Result<Json<HistoryResponse>, AppError> {
    let _workspace_id = workspace_from_headers_or_query(&headers, params.workspace_id)?;
    let svc = state.valuation_service();
    let limit = params.limit.unwrap_or(90).clamp(1, 365);

    let rows = svc.history(&params.printing_id, limit).await?;
    let data = rows
        .into_iter()
        .map(|r| HistoryPoint {
            price_usd: r.price,
            source: r.source,
            captured_at: r.captured_at,
        })
        .collect();

    Ok(Json(HistoryResponse {
        printing_id: params.printing_id,
        data,
    }))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::HeaderName;
    use axum::http::HeaderValue;

    #[test]
    fn workspace_from_header_parses_uuid() {
        let id = Uuid::new_v4();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-workspace-id"),
            HeaderValue::from_str(&id.to_string()).unwrap(),
        );
        assert_eq!(workspace_from_headers_or_query(&headers, None).unwrap(), id);
    }

    #[test]
    fn workspace_from_query_param() {
        let id = Uuid::new_v4();
        let headers = HeaderMap::new();
        assert_eq!(workspace_from_headers_or_query(&headers, Some(id)).unwrap(), id);
    }

    #[test]
    fn workspace_missing_returns_bad_request() {
        let headers = HeaderMap::new();
        assert!(workspace_from_headers_or_query(&headers, None).is_err());
    }
}
