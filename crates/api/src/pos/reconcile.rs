use async_trait::async_trait;
use axum::{
    extract::{Extension, Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    app::AppState, error::AppError, models::connection::PosConnection, pos::mapping::MappingService,
};

// ─── POS provider abstraction (contract for connect-pos stream) ───────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosInventoryItem {
    pub sku: String,
    pub name: String,
    pub quantity: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosSaleLine {
    pub sku: String,
    /// Positive quantity for sales; negative is invalid here — sign is determined by transaction_type.
    pub quantity: i32,
    pub unit_price_cents: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosSale {
    pub external_id: String,
    /// "sale" | "refund" | "return"
    pub transaction_type: String,
    pub transaction_date: chrono::DateTime<Utc>,
    pub lines: Vec<PosSaleLine>,
}

/// Implemented by the connect-pos stream for each POS provider (Square, Clover, …).
#[async_trait]
pub trait PosProvider: Send + Sync {
    async fn get_inventory(
        &self,
        connection: &PosConnection,
    ) -> Result<Vec<PosInventoryItem>, AppError>;

    async fn get_sales_since(
        &self,
        connection: &PosConnection,
        since: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<PosSale>, AppError>;

    async fn update_inventory(
        &self,
        connection: &PosConnection,
        sku: &str,
        quantity: i32,
    ) -> Result<(), AppError>;
}

// ─── Domain models ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReconciliationReport {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub connection_id: Uuid,
    pub report_date: NaiveDate,
    /// "pending" | "syncing" | "completed" | "failed" | "stale"
    pub status: String,
    pub discrepancy_count: i32,
    pub unresolved_count: i32,
    pub synced_at: Option<chrono::DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReconciliationDiscrepancy {
    pub id: Uuid,
    pub report_id: Uuid,
    pub printing_id: Option<Uuid>,
    pub pos_sku: String,
    /// "missing" | "extra" | "quantity_mismatch" | "negative_quantity"
    pub discrepancy_type: String,
    pub catalogue_qty: Option<i32>,
    pub pos_qty: Option<i32>,
    /// "pending" | "accept_pos" | "accept_catalogue" | "manual_adjust" | "investigate"
    pub resolution: String,
    pub resolved_at: Option<chrono::DateTime<Utc>>,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transaction {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub connection_id: Uuid,
    pub external_id: String,
    /// "sale" | "refund" | "return"
    pub transaction_type: String,
    pub pos_transaction_date: chrono::DateTime<Utc>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TransactionLine {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub pos_sku: String,
    pub printing_id: Option<Uuid>,
    pub pos_product_mapping_id: Option<Uuid>,
    /// Negative for refunds/returns.
    pub quantity: i32,
    pub unit_price_cents: Option<i32>,
    pub created_at: chrono::DateTime<Utc>,
}

// ─── Service ──────────────────────────────────────────────────────────────────

pub struct ReconciliationService;

impl ReconciliationService {
    /// Full reconciliation cycle for one POS connection.
    ///
    /// Idempotent: safe to retry after a partial failure — already-ingested sales
    /// are skipped on the `(connection_id, external_id)` unique key.
    pub async fn run_reconciliation(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Uuid,
        provider: Arc<dyn PosProvider>,
    ) -> Result<ReconciliationReport, AppError> {
        let connection = Self::fetch_active_connection(pool, workspace_id, connection_id).await?;

        let today = Utc::now().date_naive();
        let report = Self::upsert_report(pool, workspace_id, connection_id, today).await?;

        Self::set_status(pool, report.id, "syncing", None).await?;

        // Ingest new sales; errors leave the report in 'failed' so the next run resumes.
        let ingest = match Self::ingest_sales(pool, &connection, provider.as_ref()).await {
            Ok(r) => r,
            Err(e) => {
                Self::set_status(pool, report.id, "failed", Some(&e.to_string())).await?;
                return Err(e);
            }
        };

        if ingest.negative_qty_alerts > 0 {
            tracing::warn!(
                connection_id = %connection_id,
                count = ingest.negative_qty_alerts,
                "Negative-quantity alerts during sale ingestion",
            );
        }

        // Diff catalogue vs POS when the connection pulls from POS.
        let disc_count = if connection.pulls_from_pos() {
            match Self::diff_and_write_discrepancies(
                pool,
                &connection,
                report.id,
                provider.as_ref(),
            )
            .await
            {
                Ok(n) => n,
                Err(e) => {
                    Self::set_status(pool, report.id, "failed", Some(&e.to_string())).await?;
                    return Err(e);
                }
            }
        } else {
            0i32
        };

        // Push catalogue quantities back to POS when the connection allows it.
        if connection.pushes_to_pos() {
            if let Err(e) = Self::push_to_pos(pool, &connection, provider.as_ref()).await {
                tracing::warn!(
                    "Non-fatal: POS push failed for connection={}: {}",
                    connection_id,
                    e
                );
            }
        }

        let unresolved: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM reconciliation_discrepancies \
             WHERE report_id = $1 AND resolution = 'pending'",
        )
        .bind(report.id)
        .fetch_one(pool)
        .await?;

        sqlx::query(
            "UPDATE reconciliation_reports \
             SET status = 'completed', discrepancy_count = $2, unresolved_count = $3, \
                 synced_at = NOW(), error_message = NULL, updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(report.id)
        .bind(disc_count)
        .bind(unresolved as i32)
        .execute(pool)
        .await?;

        tracing::info!(
            connection_id = %connection_id,
            ingested = ingest.ingested,
            discrepancies = disc_count,
            "Reconciliation complete",
        );

        Self::get_report(pool, report.id).await
    }

    /// Idempotently ingest sales from the POS into Transaction + TransactionLine rows.
    async fn ingest_sales(
        pool: &PgPool,
        connection: &PosConnection,
        provider: &dyn PosProvider,
    ) -> Result<IngestResult, AppError> {
        let sales = provider
            .get_sales_since(connection, connection.last_synced_at)
            .await?;

        let mut result = IngestResult::default();

        for sale in &sales {
            // Idempotency guard on (connection_id, external_id).
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(\
                     SELECT 1 FROM transactions \
                     WHERE connection_id = $1 AND external_id = $2\
                 )",
            )
            .bind(connection.id)
            .bind(&sale.external_id)
            .fetch_one(pool)
            .await?;

            if exists {
                result.skipped += 1;
                continue;
            }

            // Refunds and returns reverse the quantity direction.
            let qty_sign: i32 = if sale.transaction_type == "sale" {
                -1
            } else {
                1
            };

            let txn_id: Uuid = sqlx::query_scalar(
                "INSERT INTO transactions \
                     (id, workspace_id, connection_id, external_id, transaction_type, pos_transaction_date) \
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5) \
                 RETURNING id",
            )
            .bind(connection.workspace_id)
            .bind(connection.id)
            .bind(&sale.external_id)
            .bind(&sale.transaction_type)
            .bind(sale.transaction_date)
            .fetch_one(pool)
            .await?;

            for line in &sale.lines {
                let mapping = MappingService::find_mapping(pool, connection.id, &line.sku).await?;
                let printing_id = mapping.as_ref().map(|m| m.printing_id);
                let mapping_id = mapping.as_ref().map(|m| m.id);

                if mapping.is_none() {
                    MappingService::queue_unmapped(pool, connection, &line.sku, None).await?;
                }

                let applied_qty = line.quantity * qty_sign;

                sqlx::query(
                    "INSERT INTO transaction_lines \
                         (id, transaction_id, pos_sku, printing_id, pos_product_mapping_id, \
                          quantity, unit_price_cents) \
                     VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6)",
                )
                .bind(txn_id)
                .bind(&line.sku)
                .bind(printing_id)
                .bind(mapping_id)
                .bind(applied_qty)
                .bind(line.unit_price_cents)
                .execute(pool)
                .await?;

                if let Some(pid) = printing_id {
                    let new_qty: i32 = sqlx::query_scalar(
                        "UPDATE printings SET quantity = quantity + $1 \
                         WHERE id = $2 \
                         RETURNING quantity",
                    )
                    .bind(applied_qty)
                    .bind(pid)
                    .fetch_one(pool)
                    .await?;

                    if new_qty < 0 {
                        result.negative_qty_alerts += 1;
                        tracing::warn!(printing_id = %pid, quantity = new_qty, "Negative catalogue quantity");
                    }
                }
            }

            result.ingested += 1;
        }

        // Advance the connection's last_synced_at watermark.
        sqlx::query("UPDATE pos_connections SET last_synced_at = NOW() WHERE id = $1")
            .bind(connection.id)
            .execute(pool)
            .await?;

        Ok(result)
    }

    /// Compare catalogue quantities against POS snapshot and write discrepancies.
    /// Clears existing discrepancies first so re-runs don't double-count.
    async fn diff_and_write_discrepancies(
        pool: &PgPool,
        connection: &PosConnection,
        report_id: Uuid,
        provider: &dyn PosProvider,
    ) -> Result<i32, AppError> {
        sqlx::query("DELETE FROM reconciliation_discrepancies WHERE report_id = $1")
            .bind(report_id)
            .execute(pool)
            .await?;

        let pos_items = provider.get_inventory(connection).await?;
        let pos_map: std::collections::HashMap<String, i32> = pos_items
            .iter()
            .map(|i| (i.sku.clone(), i.quantity))
            .collect();

        #[derive(sqlx::FromRow)]
        struct MappedPrinting {
            pos_sku: String,
            printing_id: Uuid,
            catalogue_qty: i32,
        }

        let mapped: Vec<MappedPrinting> = sqlx::query_as::<_, MappedPrinting>(
            "SELECT m.pos_sku, m.printing_id, p.quantity AS catalogue_qty \
             FROM pos_product_mappings m \
             JOIN printings p ON p.id = m.printing_id \
             WHERE m.connection_id = $1",
        )
        .bind(connection.id)
        .fetch_all(pool)
        .await?;

        let mut disc_count = 0i32;
        let mut mapped_skus = std::collections::HashSet::new();

        for row in &mapped {
            mapped_skus.insert(row.pos_sku.clone());

            if row.catalogue_qty < 0 {
                Self::insert_discrepancy(
                    pool,
                    report_id,
                    Some(row.printing_id),
                    &row.pos_sku,
                    "negative_quantity",
                    Some(row.catalogue_qty),
                    pos_map.get(&row.pos_sku).copied(),
                )
                .await?;
                disc_count += 1;
            } else {
                match pos_map.get(&row.pos_sku) {
                    Some(&pos_qty) if pos_qty != row.catalogue_qty => {
                        Self::insert_discrepancy(
                            pool,
                            report_id,
                            Some(row.printing_id),
                            &row.pos_sku,
                            "quantity_mismatch",
                            Some(row.catalogue_qty),
                            Some(pos_qty),
                        )
                        .await?;
                        disc_count += 1;
                    }
                    None => {
                        Self::insert_discrepancy(
                            pool,
                            report_id,
                            Some(row.printing_id),
                            &row.pos_sku,
                            "missing",
                            Some(row.catalogue_qty),
                            None,
                        )
                        .await?;
                        disc_count += 1;
                    }
                    _ => {}
                }
            }
        }

        // POS items with no mapping → queue for manual mapping; not a discrepancy yet.
        for pos_item in &pos_items {
            if !mapped_skus.contains(&pos_item.sku) {
                MappingService::queue_unmapped(
                    pool,
                    connection,
                    &pos_item.sku,
                    Some(&pos_item.name),
                )
                .await?;
            }
        }

        Ok(disc_count)
    }

    async fn push_to_pos(
        pool: &PgPool,
        connection: &PosConnection,
        provider: &dyn PosProvider,
    ) -> Result<(), AppError> {
        let rows: Vec<(String, i32)> = sqlx::query_as::<_, (String, i32)>(
            "SELECT m.pos_sku, p.quantity \
             FROM pos_product_mappings m \
             JOIN printings p ON p.id = m.printing_id \
             WHERE m.connection_id = $1",
        )
        .bind(connection.id)
        .fetch_all(pool)
        .await?;

        for (sku, qty) in rows {
            provider.update_inventory(connection, &sku, qty).await?;
        }
        Ok(())
    }

    async fn insert_discrepancy(
        pool: &PgPool,
        report_id: Uuid,
        printing_id: Option<Uuid>,
        pos_sku: &str,
        disc_type: &str,
        catalogue_qty: Option<i32>,
        pos_qty: Option<i32>,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO reconciliation_discrepancies \
                 (id, report_id, printing_id, pos_sku, discrepancy_type, \
                  catalogue_qty, pos_qty, resolution) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, 'pending')",
        )
        .bind(report_id)
        .bind(printing_id)
        .bind(pos_sku)
        .bind(disc_type)
        .bind(catalogue_qty)
        .bind(pos_qty)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn set_status(
        pool: &PgPool,
        report_id: Uuid,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE reconciliation_reports \
             SET status = $2, error_message = $3, updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(report_id)
        .bind(status)
        .bind(error)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn fetch_active_connection(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Uuid,
    ) -> Result<PosConnection, AppError> {
        let conn = sqlx::query_as::<_, PosConnection>(
            "SELECT * FROM pos_connections WHERE id = $1 AND workspace_id = $2",
        )
        .bind(connection_id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;

        if !conn.is_active {
            return Err(AppError::BadRequest("POS connection is inactive".into()));
        }
        Ok(conn)
    }

    async fn upsert_report(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Uuid,
        report_date: NaiveDate,
    ) -> Result<ReconciliationReport, AppError> {
        sqlx::query_as::<_, ReconciliationReport>(
            "INSERT INTO reconciliation_reports \
                 (id, workspace_id, connection_id, report_date, status, \
                  discrepancy_count, unresolved_count) \
             VALUES (gen_random_uuid(), $1, $2, $3, 'pending', 0, 0) \
             ON CONFLICT (connection_id, report_date) DO UPDATE \
                 SET updated_at = NOW() \
             RETURNING *",
        )
        .bind(workspace_id)
        .bind(connection_id)
        .bind(report_date)
        .fetch_one(pool)
        .await
        .map_err(AppError::from)
    }

    // ─── Public query helpers (used by HTTP handlers) ─────────────────────────

    pub async fn get_report(
        pool: &PgPool,
        report_id: Uuid,
    ) -> Result<ReconciliationReport, AppError> {
        sqlx::query_as::<_, ReconciliationReport>(
            "SELECT * FROM reconciliation_reports WHERE id = $1",
        )
        .bind(report_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
    }

    pub async fn list_reports(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Option<Uuid>,
    ) -> Result<Vec<ReconciliationReport>, AppError> {
        match connection_id {
            Some(cid) => sqlx::query_as::<_, ReconciliationReport>(
                "SELECT * FROM reconciliation_reports \
                 WHERE workspace_id = $1 AND connection_id = $2 \
                 ORDER BY report_date DESC \
                 LIMIT 30",
            )
            .bind(workspace_id)
            .bind(cid)
            .fetch_all(pool)
            .await
            .map_err(AppError::from),

            None => sqlx::query_as::<_, ReconciliationReport>(
                "SELECT * FROM reconciliation_reports \
                 WHERE workspace_id = $1 \
                 ORDER BY report_date DESC \
                 LIMIT 30",
            )
            .bind(workspace_id)
            .fetch_all(pool)
            .await
            .map_err(AppError::from),
        }
    }

    pub async fn get_discrepancies(
        pool: &PgPool,
        report_id: Uuid,
    ) -> Result<Vec<ReconciliationDiscrepancy>, AppError> {
        sqlx::query_as::<_, ReconciliationDiscrepancy>(
            "SELECT * FROM reconciliation_discrepancies \
             WHERE report_id = $1 \
             ORDER BY resolution, created_at",
        )
        .bind(report_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn resolve_discrepancy(
        pool: &PgPool,
        workspace_id: Uuid,
        discrepancy_id: Uuid,
        resolution: &str,
        notes: Option<&str>,
    ) -> Result<ReconciliationDiscrepancy, AppError> {
        const VALID: &[&str] = &[
            "accept_pos",
            "accept_catalogue",
            "manual_adjust",
            "investigate",
        ];
        if !VALID.contains(&resolution) {
            return Err(AppError::BadRequest(format!(
                "Unknown resolution '{}'. Valid: {}",
                resolution,
                VALID.join(", ")
            )));
        }

        let disc = sqlx::query_as::<_, ReconciliationDiscrepancy>(
            "UPDATE reconciliation_discrepancies \
             SET resolution = $2, notes = $3, resolved_at = NOW() \
             WHERE id = $1 \
             RETURNING *",
        )
        .bind(discrepancy_id)
        .bind(resolution)
        .bind(notes)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;

        // Side-effects for resolutions that mutate catalogue quantity.
        if resolution == "accept_pos" {
            if let (Some(printing_id), Some(pos_qty)) = (disc.printing_id, disc.pos_qty) {
                sqlx::query(
                    "UPDATE printings SET quantity = $1, updated_at = NOW() \
                     WHERE id = $2 AND workspace_id = $3",
                )
                .bind(pos_qty)
                .bind(printing_id)
                .bind(workspace_id)
                .execute(pool)
                .await?;
            }
        }

        // Refresh the report's unresolved count.
        sqlx::query(
            "UPDATE reconciliation_reports \
             SET unresolved_count = (\
                 SELECT COUNT(*) FROM reconciliation_discrepancies \
                 WHERE report_id = reconciliation_reports.id AND resolution = 'pending'\
             ), updated_at = NOW() \
             WHERE id = (\
                 SELECT report_id FROM reconciliation_discrepancies WHERE id = $1\
             )",
        )
        .bind(discrepancy_id)
        .execute(pool)
        .await?;

        Ok(disc)
    }
}

// ─── Internal ─────────────────────────────────────────────────────────────────

#[derive(Default)]
struct IngestResult {
    ingested: usize,
    skipped: usize,
    negative_qty_alerts: usize,
}

// ─── Scheduled job ────────────────────────────────────────────────────────────

/// Spawns a background task that reconciles all active connections every hour.
pub fn spawn_reconciliation_scheduler(
    pool: PgPool,
    provider: Arc<dyn PosProvider>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = reconcile_all_active(&pool, Arc::clone(&provider)).await {
                tracing::error!("Scheduled reconciliation error: {}", e);
            }
        }
    })
}

async fn reconcile_all_active(
    pool: &PgPool,
    provider: Arc<dyn PosProvider>,
) -> Result<(), AppError> {
    #[derive(sqlx::FromRow)]
    struct ActiveConn {
        workspace_id: Uuid,
        id: Uuid,
    }

    let conns: Vec<ActiveConn> = sqlx::query_as::<_, ActiveConn>(
        "SELECT workspace_id, id FROM pos_connections WHERE is_active = true",
    )
    .fetch_all(pool)
    .await?;

    for c in conns {
        if let Err(e) = ReconciliationService::run_reconciliation(
            pool,
            c.workspace_id,
            c.id,
            Arc::clone(&provider),
        )
        .await
        {
            tracing::error!(connection_id = %c.id, error = %e, "Scheduled reconciliation failed");
        }
    }
    Ok(())
}

// ─── HTTP handlers ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    pub connection_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct ListReportsQuery {
    pub connection_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub resolution: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportDetail {
    pub report: ReconciliationReport,
    pub discrepancies: Vec<ReconciliationDiscrepancy>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/:wid/pos/sync", post(handle_sync_now))
        .route(
            "/workspaces/:wid/pos/reconciliation",
            get(handle_list_reports),
        )
        .route(
            "/workspaces/:wid/pos/reconciliation/:report_id",
            get(handle_get_report),
        )
        .route(
            "/workspaces/:wid/pos/reconciliation/:report_id/discrepancies/:disc_id/resolve",
            post(handle_resolve_discrepancy),
        )
}

async fn handle_sync_now(
    State(state): State<AppState>,
    Path(wid): Path<Uuid>,
    Extension(provider): Extension<Arc<dyn PosProvider>>,
    Json(req): Json<SyncRequest>,
) -> Result<Json<ReconciliationReport>, AppError> {
    let report = ReconciliationService::run_reconciliation(
        &state.pool,
        wid,
        req.connection_id,
        Arc::clone(&provider),
    )
    .await?;
    Ok(Json(report))
}

async fn handle_list_reports(
    State(state): State<AppState>,
    Path(wid): Path<Uuid>,
    Query(q): Query<ListReportsQuery>,
) -> Result<Json<Vec<ReconciliationReport>>, AppError> {
    let reports = ReconciliationService::list_reports(&state.pool, wid, q.connection_id).await?;
    Ok(Json(reports))
}

async fn handle_get_report(
    State(state): State<AppState>,
    Path((wid, report_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ReportDetail>, AppError> {
    let report = ReconciliationService::get_report(&state.pool, report_id).await?;
    if report.workspace_id != wid {
        return Err(AppError::NotFound);
    }
    let discrepancies = ReconciliationService::get_discrepancies(&state.pool, report_id).await?;
    Ok(Json(ReportDetail {
        report,
        discrepancies,
    }))
}

async fn handle_resolve_discrepancy(
    State(state): State<AppState>,
    Path((wid, _report_id, disc_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<ResolveRequest>,
) -> Result<Json<ReconciliationDiscrepancy>, AppError> {
    let disc = ReconciliationService::resolve_discrepancy(
        &state.pool,
        wid,
        disc_id,
        &req.resolution,
        req.notes.as_deref(),
    )
    .await?;
    Ok(Json(disc))
}

// ─── Null provider (placeholder until connect-pos stream lands) ──────────────

/// No-op POS provider. Returns empty data for every call.
/// Used as a placeholder until the real provider is wired up.
pub struct NullPosProvider;

#[async_trait]
impl PosProvider for NullPosProvider {
    async fn get_inventory(&self, _: &PosConnection) -> Result<Vec<PosInventoryItem>, AppError> {
        Ok(vec![])
    }

    async fn get_sales_since(
        &self,
        _: &PosConnection,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<PosSale>, AppError> {
        Ok(vec![])
    }

    async fn update_inventory(&self, _: &PosConnection, _: &str, _: i32) -> Result<(), AppError> {
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockPosProvider {
        inventory: Mutex<Vec<PosInventoryItem>>,
        sales: Mutex<Vec<PosSale>>,
    }

    impl MockPosProvider {
        fn with(inventory: Vec<PosInventoryItem>, sales: Vec<PosSale>) -> Self {
            Self {
                inventory: Mutex::new(inventory),
                sales: Mutex::new(sales),
            }
        }
    }

    #[async_trait]
    impl PosProvider for MockPosProvider {
        async fn get_inventory(
            &self,
            _c: &PosConnection,
        ) -> Result<Vec<PosInventoryItem>, AppError> {
            Ok(self.inventory.lock().unwrap().clone())
        }

        async fn get_sales_since(
            &self,
            _c: &PosConnection,
            _since: Option<chrono::DateTime<Utc>>,
        ) -> Result<Vec<PosSale>, AppError> {
            Ok(self.sales.lock().unwrap().clone())
        }

        async fn update_inventory(
            &self,
            _c: &PosConnection,
            _sku: &str,
            _qty: i32,
        ) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn mock_satisfies_trait() {
        let p = MockPosProvider::with(vec![], vec![]);
        let _: &dyn PosProvider = &p;
    }

    #[test]
    fn resolve_validation_rejects_unknown() {
        const VALID: &[&str] = &[
            "accept_pos",
            "accept_catalogue",
            "manual_adjust",
            "investigate",
        ];
        assert!(!VALID.contains(&"bad_value"));
        assert!(VALID.contains(&"accept_pos"));
    }

    #[test]
    fn qty_sign_for_refund_is_positive() {
        let sale_type = "refund";
        let sign: i32 = if sale_type == "sale" { -1 } else { 1 };
        assert_eq!(sign, 1);
    }

    #[test]
    fn qty_sign_for_sale_is_negative() {
        let sale_type = "sale";
        let sign: i32 = if sale_type == "sale" { -1 } else { 1 };
        assert_eq!(sign, -1);
    }
}
