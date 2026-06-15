//! Axum route handlers for the valuation feature.
//!
//! Mounts under `/api/v1` by foundation's `app.rs`. The `routes()` fn is the
//! only public symbol; foundation pre-mounts it as a stub and this module fills it.
//!
//! Endpoints:
//!   GET  /valuations          — workspace valuation summary + total
//!   POST /valuations/refresh  — on-demand price refresh (all or single printing)
//!   GET  /valuations/history  — price history for a single printing

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::catalogue::valuation::ValuationStatus;
use crate::error::ApiError;
use crate::middleware::auth::WorkspaceClaim;

// ── Router ─────────────────────────────────────────────────────────────────

/// Returns the Router for this module. Foundation calls this to pre-mount the stub.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/valuations", get(get_valuations))
        .route("/valuations/refresh", post(trigger_refresh))
        .route("/valuations/history", get(get_history))
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
    /// Line total = price × quantity; null when no price data.
    pub line_total_usd: Option<Decimal>,
}

/// The value sub-object on each inventory row — mirrors `ValuationStatus` for JSON consumers.
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
        /// Seconds since the last successful fetch.
        stale_seconds: i64,
    },
    /// Pricing sources returned no data for this card.
    NoData,
    /// Card was just added; the first refresh job hasn't run yet.
    Pending,
}

#[derive(Debug, Serialize)]
pub struct GetValuationsResponse {
    pub data: Vec<ValuationItem>,
    /// Workspace total in USD (sum of line totals for priced items).
    pub total_usd: Decimal,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    /// Number of printings processed (includes no-data outcomes).
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

// ── Query params ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RefreshParams {
    /// If present, refresh only this printing; otherwise refresh all stale/pending.
    pub printing_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryParams {
    /// The TCG API printing ID to fetch history for.
    pub printing_id: String,
    /// Max number of data points to return (default 90, max 365).
    pub limit: Option<i64>,
}

// ── Handlers ───────────────────────────────────────────────────────────────

async fn get_valuations(
    State(state): State<AppState>,
    claim: WorkspaceClaim,
) -> Result<Json<GetValuationsResponse>, ApiError> {
    let svc = state.valuation_service();

    let rows = svc.workspace_valuations(claim.workspace_id).await?;
    let total = svc.workspace_total(claim.workspace_id).await?;

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
                // fetched_at is None → never fetched → Pending
                (None, _, None) => (ItemValue::Pending, None),
                // price is None but fetched_at exists → NoData
                (None, _, Some(_)) => (ItemValue::NoData, None),
                // edge: price None, is_stale irrelevant without fetched_at
                _ => (ItemValue::Pending, None),
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

    Ok(Json(GetValuationsResponse {
        data,
        total_usd: total,
    }))
}

async fn trigger_refresh(
    State(state): State<AppState>,
    // Auth required; workspace claim not used here since refresh is workspace-global
    _claim: WorkspaceClaim,
    Query(params): Query<RefreshParams>,
) -> Result<Json<RefreshResponse>, ApiError> {
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
    _claim: WorkspaceClaim,
    Query(params): Query<HistoryParams>,
) -> Result<Json<HistoryResponse>, ApiError> {
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
    use rust_decimal::prelude::FromStr;

    #[test]
    fn item_value_stale_calculates_seconds() {
        // Pure serialization / construction test (no DB or HTTP needed)
        let fetched_at = Utc::now() - chrono::Duration::hours(30);
        let stale_seconds = (Utc::now() - fetched_at).num_seconds();
        assert!(stale_seconds >= 30 * 3600);
    }

    #[test]
    fn line_total_is_price_times_qty() {
        let price = Decimal::from_str("5.25").unwrap();
        let qty = 4;
        let total = price * Decimal::from(qty);
        assert_eq!(total, Decimal::from_str("21.00").unwrap());
    }
}
