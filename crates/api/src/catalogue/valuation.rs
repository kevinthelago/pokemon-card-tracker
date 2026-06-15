//! Valuation service — market price resolution, caching, and history.
//!
//! Pricing priority:
//!   1. TCGplayer market price embedded in the TCG API response (stored in `printings.tcg_prices_json`).
//!   2. PriceCharting raw/loose price as fallback.
//!
//! Graded cards: attempt PriceCharting grade-specific price (PSA only); fall back to base
//! printing price with `is_fallback = true` so callers can surface a note to the user.
//!
//! Staleness: a valuation older than 25 hours is marked Stale but still served (API-down
//! resilience). The `refresh_all` job is run daily by apalis.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};
use uuid::Uuid;

use crate::integrations::pricecharting::PriceChartingClient;

/// Hours before a cached valuation is considered stale (25h = daily refresh + 1h buffer).
const STALE_HOURS: i64 = 25;

// ── Value types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValuationSource {
    Tcgplayer,
    Pricecharting,
}

impl std::fmt::Display for ValuationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcgplayer => write!(f, "tcgplayer"),
            Self::Pricecharting => write!(f, "pricecharting"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPrice {
    pub price: Decimal,
    pub source: ValuationSource,
    pub currency: String,
    pub is_stale: bool,
    pub fetched_at: DateTime<Utc>,
}

/// The system's knowledge of a printing's market value at query time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ValuationStatus {
    /// Fresh cached price (< 25 h old).
    Current(ResolvedPrice),
    /// Last known price; > 25 h old (API may have been down during last refresh).
    Stale(ResolvedPrice),
    /// Pricing sources returned no data for this printing.
    NoData,
    /// Card was just added; the first refresh job hasn't run yet.
    Pending,
}

impl ValuationStatus {
    pub fn price(&self) -> Option<Decimal> {
        match self {
            Self::Current(r) | Self::Stale(r) => Some(r.price),
            _ => None,
        }
    }
}

// ── Apalis job payload ─────────────────────────────────────────────────────

/// Payload for a per-printing valuation refresh job.
/// Enqueued by the catalogue service after a card is added, and by the daily cron job.
/// The `apalis::prelude::Job` impl will be added when the foundation wires in apalis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationRefreshJob {
    pub printing_id: String,
}

// ── DB row types ───────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct ValuationRow {
    #[allow(dead_code)]
    id: Uuid,
    printing_id: String,
    source: String,
    price: Decimal,
    currency: String,
    fetched_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ValuationSnapshotRow {
    pub id: Uuid,
    pub printing_id: String,
    pub price: Decimal,
    pub currency: String,
    pub source: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct PrintingRow {
    id: String,
    name: String,
    set_code: String,
    /// JSON blob of TCGplayer prices fetched during identity resolution.
    /// Shape: { "normal"?: { market: f64 }, "holofoil"?: { market: f64 }, ... }
    tcg_prices_json: Option<serde_json::Value>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct WorkspaceValuationRow {
    pub printing_id: String,
    pub card_name: String,
    pub set_code: String,
    pub condition: String,
    pub quantity: i32,
    pub price: Option<Decimal>,
    pub source: Option<String>,
    pub currency: Option<String>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub is_stale: bool,
}

// ── TCG API embedded price shapes ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TcgPricesBlob {
    normal: Option<TcgPriceTier>,
    holofoil: Option<TcgPriceTier>,
    #[serde(rename = "reverseHolofoil")]
    reverse_holofoil: Option<TcgPriceTier>,
    #[serde(rename = "1stEditionNormal")]
    first_edition_normal: Option<TcgPriceTier>,
    #[serde(rename = "1stEditionHolofoil")]
    first_edition_holofoil: Option<TcgPriceTier>,
}

#[derive(Debug, Deserialize)]
struct TcgPriceTier {
    market: Option<f64>,
}

// ── Service ────────────────────────────────────────────────────────────────

pub struct ValuationService {
    pool: PgPool,
    pc: PriceChartingClient,
}

impl ValuationService {
    pub fn new(pool: PgPool, pc: PriceChartingClient) -> Self {
        Self { pool, pc }
    }

    /// Current valuation status for a printing (no external call; DB-only).
    pub async fn status(&self, printing_id: &str) -> Result<ValuationStatus> {
        let row = sqlx::query_as::<_, ValuationRow>(
            "SELECT id, printing_id, source, price, currency, fetched_at
             FROM valuations WHERE printing_id = $1",
        )
        .bind(printing_id)
        .fetch_optional(&self.pool)
        .await
        .context("query valuations")?;

        Ok(match row {
            None => ValuationStatus::Pending,
            Some(r) => {
                let age = Utc::now() - r.fetched_at;
                let resolved = ResolvedPrice {
                    price: r.price,
                    source: parse_source(&r.source),
                    currency: r.currency,
                    is_stale: age > Duration::hours(STALE_HOURS),
                    fetched_at: r.fetched_at,
                };
                if age > Duration::hours(STALE_HOURS) {
                    ValuationStatus::Stale(resolved)
                } else {
                    ValuationStatus::Current(resolved)
                }
            }
        })
    }

    /// Valuation for a graded CardInstance.
    ///
    /// Returns `(status, is_fallback)` where `is_fallback = true` means grade-specific
    /// pricing was unavailable and the base printing price was used instead.
    pub async fn graded_status(
        &self,
        printing_id: &str,
        grader: &str,
        grade: f32,
    ) -> Result<(ValuationStatus, bool)> {
        let printing = self.fetch_printing(printing_id).await?;
        let Some(printing) = printing else {
            return Ok((ValuationStatus::NoData, false));
        };

        // Only PSA has reliable grade-specific pricing on PriceCharting.
        if grader.to_ascii_uppercase() == "PSA" {
            let grade_int = grade.round().clamp(1.0, 10.0) as u8;
            match self
                .pc
                .graded_price(&printing.name, &printing.set_code, grade_int)
                .await
            {
                Ok(Some(price)) => {
                    return Ok((
                        ValuationStatus::Current(ResolvedPrice {
                            price,
                            source: ValuationSource::Pricecharting,
                            currency: "USD".to_string(),
                            is_stale: false,
                            fetched_at: Utc::now(),
                        }),
                        false,
                    ));
                }
                Ok(None) => {} // fall through to base price
                Err(e) => {
                    warn!(%printing_id, grader, grade, ?e, "PriceCharting graded lookup failed");
                }
            }
        }

        // Fallback: base printing price (CGC/BGS, or PSA with no grade data)
        let base = self.status(printing_id).await?;
        Ok((base, true))
    }

    /// Fetch fresh prices from external sources; upsert the valuation row; record a snapshot.
    pub async fn refresh(&self, printing_id: &str) -> Result<ValuationStatus> {
        let Some(printing) = self.fetch_printing(printing_id).await? else {
            warn!(%printing_id, "Printing not found during refresh");
            return Ok(ValuationStatus::NoData);
        };

        let resolved = self.resolve_price(&printing).await?;
        let Some((price, source)) = resolved else {
            info!(%printing_id, "No market data found from any source");
            return Ok(ValuationStatus::NoData);
        };

        let now = Utc::now();

        sqlx::query(
            "INSERT INTO valuations (id, printing_id, source, price, currency, fetched_at)
             VALUES ($1, $2, $3, $4, 'USD', $5)
             ON CONFLICT (printing_id) DO UPDATE
             SET source     = EXCLUDED.source,
                 price      = EXCLUDED.price,
                 currency   = EXCLUDED.currency,
                 fetched_at = EXCLUDED.fetched_at",
        )
        .bind(Uuid::new_v4())
        .bind(printing_id)
        .bind(source.to_string())
        .bind(price)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("upsert valuation")?;

        sqlx::query(
            "INSERT INTO valuation_snapshots (id, printing_id, price, currency, source, captured_at)
             VALUES ($1, $2, $3, 'USD', $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(printing_id)
        .bind(price)
        .bind(source.to_string())
        .bind(now)
        .execute(&self.pool)
        .await
        .context("insert snapshot")?;

        info!(%printing_id, %price, %source, "Valuation refreshed");
        Ok(ValuationStatus::Current(ResolvedPrice {
            price,
            source,
            currency: "USD".to_string(),
            is_stale: false,
            fetched_at: now,
        }))
    }

    /// Refresh all stale or un-priced printings (daily batch job body).
    ///
    /// Returns the number of printings processed (including those that still returned no data).
    pub async fn refresh_all(&self) -> Result<u32> {
        let cutoff = Utc::now() - Duration::hours(STALE_HOURS);

        let printing_ids: Vec<String> = sqlx::query_scalar(
            "SELECT p.id FROM printings p
             WHERE NOT EXISTS (
                 SELECT 1 FROM valuations v
                 WHERE v.printing_id = p.id AND v.fetched_at > $1
             )",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .context("fetch stale/pending printings")?;

        let count = printing_ids.len() as u32;
        info!(%count, "Starting batch valuation refresh");

        for printing_id in &printing_ids {
            if let Err(e) = self.refresh(printing_id).await {
                warn!(%printing_id, ?e, "Valuation refresh failed; continuing batch");
            }
        }

        Ok(count)
    }

    /// Sum of (valuation price × quantity) for every InventoryItem in the workspace.
    ///
    /// CardInstances (graded) count as quantity = 1. Items with no valuation are excluded
    /// from the total (not counted as zero, to avoid silently under-reporting).
    pub async fn workspace_total(&self, workspace_id: Uuid) -> Result<Decimal> {
        let total: Option<Decimal> = sqlx::query_scalar(
            "SELECT COALESCE(
                 COALESCE(
                     (SELECT SUM(v.price * i.quantity)
                      FROM inventory_items i
                      JOIN valuations v ON v.printing_id = i.printing_id
                      WHERE i.workspace_id = $1),
                     0::NUMERIC
                 )
                 + COALESCE(
                     (SELECT SUM(v.price)
                      FROM card_instances ci
                      JOIN valuations v ON v.printing_id = ci.printing_id
                      WHERE ci.workspace_id = $1),
                     0::NUMERIC
                 ),
              0::NUMERIC) AS total",
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await
        .context("workspace total")?;

        Ok(total.unwrap_or(Decimal::ZERO))
    }

    /// Historical snapshots for a single printing, newest first.
    pub async fn history(
        &self,
        printing_id: &str,
        limit: i64,
    ) -> Result<Vec<ValuationSnapshotRow>> {
        sqlx::query_as::<_, ValuationSnapshotRow>(
            "SELECT id, printing_id, price, currency, source, captured_at
             FROM valuation_snapshots
             WHERE printing_id = $1
             ORDER BY captured_at DESC
             LIMIT $2",
        )
        .bind(printing_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("fetch valuation history")
    }

    /// All current valuations for a workspace, joined with inventory metadata.
    pub async fn workspace_valuations(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceValuationRow>> {
        sqlx::query_as::<_, WorkspaceValuationRow>(
            "SELECT
                 i.printing_id,
                 p.name          AS card_name,
                 p.set_code,
                 i.condition,
                 i.quantity,
                 v.price,
                 v.source,
                 v.currency,
                 v.fetched_at,
                 CASE
                     WHEN v.fetched_at IS NULL                              THEN false
                     WHEN v.fetched_at < NOW() - INTERVAL '25 hours'       THEN true
                     ELSE false
                 END             AS is_stale
             FROM inventory_items i
             JOIN printings p ON p.id = i.printing_id
             LEFT JOIN valuations v ON v.printing_id = i.printing_id
             WHERE i.workspace_id = $1
             ORDER BY p.name ASC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .context("workspace valuations")
    }

    // ── Internals ──────────────────────────────────────────────────────────

    async fn fetch_printing(&self, printing_id: &str) -> Result<Option<PrintingRow>> {
        sqlx::query_as::<_, PrintingRow>(
            "SELECT id, name, set_code, tcg_prices_json FROM printings WHERE id = $1",
        )
        .bind(printing_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetch printing")
    }

    /// Try each pricing source in priority order.
    async fn resolve_price(
        &self,
        printing: &PrintingRow,
    ) -> Result<Option<(Decimal, ValuationSource)>> {
        // 1. TCGplayer market price (embedded in identity data, no extra call needed)
        if let Some(price) = self.extract_tcg_market_price(printing) {
            return Ok(Some((price, ValuationSource::Tcgplayer)));
        }

        // 2. PriceCharting fallback
        match self.pc.raw_price(&printing.name, &printing.set_code).await {
            Ok(Some(price)) => return Ok(Some((price, ValuationSource::Pricecharting))),
            Ok(None) => {}
            Err(e) => {
                warn!(printing_id = %printing.id, ?e, "PriceCharting raw price failed");
            }
        }

        Ok(None)
    }

    /// Extract the best available TCGplayer market price from the JSON blob.
    ///
    /// Priority: holofoil > 1st-edition-holofoil > normal > reverse-holo.
    fn extract_tcg_market_price(&self, printing: &PrintingRow) -> Option<Decimal> {
        let blob: TcgPricesBlob = serde_json::from_value(printing.tcg_prices_json.clone()?).ok()?;

        let market_f64 = blob
            .holofoil
            .as_ref()
            .and_then(|t| t.market)
            .or_else(|| blob.first_edition_holofoil.as_ref().and_then(|t| t.market))
            .or_else(|| blob.normal.as_ref().and_then(|t| t.market))
            .or_else(|| blob.reverse_holofoil.as_ref().and_then(|t| t.market))?;

        // f64 → Decimal: safe for currency amounts in the card pricing range
        Decimal::try_from(market_f64).ok()
    }
}

fn parse_source(s: &str) -> ValuationSource {
    match s {
        "tcgplayer" => ValuationSource::Tcgplayer,
        _ => ValuationSource::Pricecharting,
    }
}

// ── Daily batch-refresh scheduler ─────────────────────────────────────────

/// Spawns a background task that calls `refresh_all` every 24 hours.
/// Mirrors the pattern used by `pos::reconcile::spawn_reconciliation_scheduler`.
pub fn spawn_valuation_refresh_scheduler(
    svc: Arc<ValuationService>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(24 * 3600));
        loop {
            interval.tick().await;
            match svc.refresh_all().await {
                Ok(n) => tracing::info!(count = n, "daily valuation refresh complete"),
                Err(e) => tracing::error!("daily valuation refresh failed: {e}"),
            }
        }
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::FromStr;
    use serde_json::json;

    fn tcg_price_extractor() -> impl Fn(&PrintingRow) -> Option<Decimal> {
        // Closure captures no state — mirrors the pure helper method
        |printing: &PrintingRow| {
            let blob: TcgPricesBlob =
                serde_json::from_value(printing.tcg_prices_json.clone()?).ok()?;

            let market_f64 = blob
                .holofoil
                .as_ref()
                .and_then(|t| t.market)
                .or_else(|| blob.first_edition_holofoil.as_ref().and_then(|t| t.market))
                .or_else(|| blob.normal.as_ref().and_then(|t| t.market))
                .or_else(|| blob.reverse_holofoil.as_ref().and_then(|t| t.market))?;

            Decimal::try_from(market_f64).ok()
        }
    }

    fn printing_with_tcg_prices(prices_json: serde_json::Value) -> PrintingRow {
        PrintingRow {
            id: "xy1-1".into(),
            name: "Venusaur-EX".into(),
            set_code: "XY".into(),
            tcg_prices_json: Some(prices_json),
        }
    }

    #[test]
    fn extracts_holofoil_market_price() {
        let extract = tcg_price_extractor();
        let printing = printing_with_tcg_prices(json!({
            "holofoil": { "low": 1.00, "mid": 2.50, "market": 2.15 },
            "normal":   { "low": 0.50, "mid": 0.80, "market": 0.75 }
        }));
        assert_eq!(
            extract(&printing),
            Some(Decimal::try_from(2.15_f64).unwrap())
        );
    }

    #[test]
    fn falls_back_to_normal_when_no_holofoil() {
        let extract = tcg_price_extractor();
        let printing = printing_with_tcg_prices(json!({
            "normal": { "market": 0.75 }
        }));
        assert_eq!(
            extract(&printing),
            Some(Decimal::try_from(0.75_f64).unwrap())
        );
    }

    #[test]
    fn no_tcg_prices_returns_none() {
        let extract = tcg_price_extractor();
        let printing = PrintingRow {
            id: "xy1-1".into(),
            name: "Test".into(),
            set_code: "XY".into(),
            tcg_prices_json: None,
        };
        assert!(extract(&printing).is_none());
    }

    #[test]
    fn valuation_status_price_extracts_correctly() {
        let price = Decimal::from_str("10.50").unwrap();
        let status = ValuationStatus::Current(ResolvedPrice {
            price,
            source: ValuationSource::Tcgplayer,
            currency: "USD".into(),
            is_stale: false,
            fetched_at: Utc::now(),
        });
        assert_eq!(status.price(), Some(price));
        assert_eq!(ValuationStatus::NoData.price(), None);
        assert_eq!(ValuationStatus::Pending.price(), None);
    }
}
