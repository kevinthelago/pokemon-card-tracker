// Owned by the detect-scalpers stream.
//
// Implements:
//   - Per-workspace DetectionConfig CRUD
//   - BuyerAllowlist CRUD
//   - Scalper detection engine (velocity, bulk, sweep, repeat patterns)
//   - Routes: GET/PATCH /risk/scalper-config
//             GET/POST/DELETE /risk/buyer-allowlist[/:id]
//             POST /risk/scalper-scan/:transaction_id
//
// Redis is used for O(log N) sliding-window velocity counters when available.
// When Redis is absent the engine falls back to Postgres COUNT queries, which
// are accurate but slower at high transaction rates.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::AuthUser,
    error::{AppError, Result},
    fraud::{
        flags::{self, FlagKind, FlagSeverity, TargetType},
        routes::{create_flag, require_owner, workspace_access, WorkspaceAccess},
    },
    models::workspace::WorkspaceKind,
};

// ── Detection config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DetectionConfig {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub enabled: bool,
    pub velocity_window_hours: i32,
    pub velocity_threshold: i32,
    pub bulk_single_item_limit: i32,
    pub sweep_printing_limit: i32,
    pub repeat_window_minutes: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub enabled: Option<bool>,
    pub velocity_window_hours: Option<i32>,
    pub velocity_threshold: Option<i32>,
    pub bulk_single_item_limit: Option<i32>,
    pub sweep_printing_limit: Option<i32>,
    pub repeat_window_minutes: Option<i32>,
}

pub async fn get_or_create_config(db: &PgPool, workspace_id: Uuid) -> Result<DetectionConfig> {
    // Upsert the default row on first access.
    let cfg: DetectionConfig = sqlx::query_as(
        r#"
        INSERT INTO scalper_detection_config (workspace_id)
        VALUES ($1)
        ON CONFLICT (workspace_id) DO NOTHING;

        SELECT id, workspace_id, enabled,
               velocity_window_hours, velocity_threshold,
               bulk_single_item_limit, sweep_printing_limit,
               repeat_window_minutes, updated_at
        FROM scalper_detection_config
        WHERE workspace_id = $1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(db)
    .await?;
    Ok(cfg)
}

pub async fn update_config(
    db: &PgPool,
    workspace_id: Uuid,
    req: UpdateConfigRequest,
) -> Result<DetectionConfig> {
    // Ensure the row exists before patching.
    sqlx::query(
        "INSERT INTO scalper_detection_config (workspace_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(workspace_id)
    .execute(db)
    .await?;

    let cfg: DetectionConfig = sqlx::query_as(
        r#"
        UPDATE scalper_detection_config SET
            enabled                = COALESCE($2, enabled),
            velocity_window_hours  = COALESCE($3, velocity_window_hours),
            velocity_threshold     = COALESCE($4, velocity_threshold),
            bulk_single_item_limit = COALESCE($5, bulk_single_item_limit),
            sweep_printing_limit   = COALESCE($6, sweep_printing_limit),
            repeat_window_minutes  = COALESCE($7, repeat_window_minutes),
            updated_at             = NOW()
        WHERE workspace_id = $1
        RETURNING id, workspace_id, enabled,
                  velocity_window_hours, velocity_threshold,
                  bulk_single_item_limit, sweep_printing_limit,
                  repeat_window_minutes, updated_at
        "#,
    )
    .bind(workspace_id)
    .bind(req.enabled)
    .bind(req.velocity_window_hours)
    .bind(req.velocity_threshold)
    .bind(req.bulk_single_item_limit)
    .bind(req.sweep_printing_limit)
    .bind(req.repeat_window_minutes)
    .fetch_one(db)
    .await?;
    Ok(cfg)
}

// ── Buyer allowlist ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AllowlistEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub buyer_hash: String,
    pub notes: Option<String>,
    pub added_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AddAllowlistRequest {
    /// SHA-256 hex of the buyer identifier supplied by the POS.
    pub buyer_hash: String,
    pub notes: Option<String>,
}

pub async fn list_allowlist(db: &PgPool, workspace_id: Uuid) -> Result<Vec<AllowlistEntry>> {
    sqlx::query_as(
        "SELECT id,workspace_id,buyer_hash,notes,added_by,created_at FROM buyer_allowlist WHERE workspace_id=$1 ORDER BY created_at DESC",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map_err(Into::into)
}

pub async fn add_to_allowlist(
    db: &PgPool,
    workspace_id: Uuid,
    buyer_hash: &str,
    notes: Option<&str>,
    added_by: Uuid,
) -> Result<AllowlistEntry> {
    let entry: AllowlistEntry = sqlx::query_as(
        r#"
        INSERT INTO buyer_allowlist (workspace_id, buyer_hash, notes, added_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (workspace_id, buyer_hash)
            DO UPDATE SET notes = EXCLUDED.notes
        RETURNING id, workspace_id, buyer_hash, notes, added_by, created_at
        "#,
    )
    .bind(workspace_id)
    .bind(buyer_hash)
    .bind(notes)
    .bind(added_by)
    .fetch_one(db)
    .await?;
    Ok(entry)
}

pub async fn remove_from_allowlist(db: &PgPool, workspace_id: Uuid, entry_id: Uuid) -> Result<()> {
    let rows = sqlx::query(
        "DELETE FROM buyer_allowlist WHERE id=$1 AND workspace_id=$2",
    )
    .bind(entry_id)
    .bind(workspace_id)
    .execute(db)
    .await?
    .rows_affected();
    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

async fn is_allowlisted(db: &PgPool, workspace_id: Uuid, buyer_hash: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM buyer_allowlist WHERE workspace_id=$1 AND buyer_hash=$2",
    )
    .bind(workspace_id)
    .bind(buyer_hash)
    .fetch_one(db)
    .await?;
    Ok(count > 0)
}

// ── Pattern definitions ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "pattern", rename_all = "snake_case")]
pub enum ScalperPattern {
    /// Buyer made more purchases than the velocity threshold in the config window.
    Velocity {
        count: i64,
        window_hours: i32,
        threshold: i32,
    },
    /// A single transaction contained more units of one item than the limit.
    BulkPurchase {
        max_quantity: i32,
        limit: i32,
        printing_id: Option<String>,
    },
    /// Buyer's total lifetime quantity of a single printing exceeds the sweep limit.
    Sweep {
        printing_id: String,
        total_quantity: i64,
        limit: i32,
    },
    /// Buyer made a second purchase within the repeat window after a prior one.
    Repeat {
        previous_purchase_at: DateTime<Utc>,
        window_minutes: i32,
    },
}

impl ScalperPattern {
    fn severity(&self) -> FlagSeverity {
        match self {
            // Velocity above threshold is high-confidence; severity scales with excess.
            Self::Velocity { count, threshold, .. } => {
                let ratio = *count as f64 / *threshold as f64;
                if ratio >= 3.0 { FlagSeverity::Critical }
                else if ratio >= 2.0 { FlagSeverity::High }
                else { FlagSeverity::Medium }
            }
            Self::BulkPurchase { max_quantity, limit, .. } => {
                if *max_quantity >= limit * 3 { FlagSeverity::Critical }
                else if *max_quantity >= limit * 2 { FlagSeverity::High }
                else { FlagSeverity::Medium }
            }
            // Sweep of limited stock is always high severity.
            Self::Sweep { .. } => FlagSeverity::High,
            // Repeat alone is low severity; it often accompanies velocity.
            Self::Repeat { .. } => FlagSeverity::Low,
        }
    }

    fn trigger_info(&self) -> String {
        match self {
            Self::Velocity { count, window_hours, threshold } =>
                format!("velocity: {} purchases in {}h (threshold {})", count, window_hours, threshold),
            Self::BulkPurchase { max_quantity, limit, printing_id } =>
                format!("bulk purchase: {} units of {} (limit {})", max_quantity,
                        printing_id.as_deref().unwrap_or("unknown"), limit),
            Self::Sweep { printing_id, total_quantity, limit } =>
                format!("sweep: {} total units of {} (limit {})", total_quantity, printing_id, limit),
            Self::Repeat { previous_purchase_at, window_minutes } =>
                format!("repeat buy within {}min of previous at {}", window_minutes, previous_purchase_at.format("%H:%M UTC")),
        }
    }
}

// ── Detection helpers ─────────────────────────────────────────────────────────

/// Row returned by the velocity Postgres fallback query.
#[derive(sqlx::FromRow)]
struct PurchaseCount {
    count: i64,
}

/// Check velocity using Redis ZSET sliding window when Redis is available,
/// or fall back to a Postgres COUNT query.
async fn check_velocity(
    db: &PgPool,
    redis: &Option<redis::aio::ConnectionManager>,
    workspace_id: Uuid,
    buyer_hash: &str,
    occurred_at: DateTime<Utc>,
    config: &DetectionConfig,
) -> Result<Option<ScalperPattern>> {
    let window_secs = config.velocity_window_hours as i64 * 3600;
    let count = if let Some(mut conn) = redis.clone() {
        // Redis sliding window: ZSET keyed by workspace+buyer_hash, scored by Unix timestamp.
        let key = format!("scalper:vel:{}:{}", workspace_id, buyer_hash);
        let now_ts = occurred_at.timestamp();
        let min_ts = now_ts - window_secs;

        // Record this transaction, then count members in the window.
        // Using pipeline to be atomic.
        let count: i64 = redis::pipe()
            .atomic()
            .cmd("ZADD").arg(&key).arg(now_ts as f64).arg(now_ts.to_string())
            .cmd("ZREMRANGEBYSCORE").arg(&key).arg("-inf").arg((min_ts - 1) as f64)
            .cmd("ZCARD").arg(&key)
            .expire(&key, window_secs as i64 + 60)
            .query_async::<Vec<redis::Value>>(&mut conn)
            .await
            .map(|v| {
                // Third command result is ZCARD (index 2)
                if let Some(redis::Value::Int(n)) = v.get(2) { *n } else { 0 }
            })
            .unwrap_or(0);
        count
    } else {
        // Postgres fallback: count transactions by this buyer in the window.
        let row: PurchaseCount = sqlx::query_as(
            r#"
            SELECT COUNT(*)::bigint AS count FROM transactions
            WHERE workspace_id = $1
              AND buyer_hash = $2
              AND occurred_at > NOW() - ($3 || ' hours')::interval
            "#,
        )
        .bind(workspace_id)
        .bind(buyer_hash)
        .bind(config.velocity_window_hours)
        .fetch_one(db)
        .await?;
        row.count
    };

    if count > config.velocity_threshold as i64 {
        Ok(Some(ScalperPattern::Velocity {
            count,
            window_hours: config.velocity_window_hours,
            threshold: config.velocity_threshold,
        }))
    } else {
        Ok(None)
    }
}

#[derive(sqlx::FromRow)]
struct BulkRow {
    max_qty: i32,
    printing_id: Option<String>,
}

async fn check_bulk(
    db: &PgPool,
    transaction_id: Uuid,
    config: &DetectionConfig,
) -> Result<Option<ScalperPattern>> {
    let row: Option<BulkRow> = sqlx::query_as(
        r#"
        SELECT MAX(quantity)::int AS max_qty, printing_id
        FROM transaction_lines
        WHERE transaction_id = $1
        GROUP BY printing_id
        ORDER BY max_qty DESC
        LIMIT 1
        "#,
    )
    .bind(transaction_id)
    .fetch_optional(db)
    .await?;

    if let Some(r) = row {
        if r.max_qty > config.bulk_single_item_limit {
            return Ok(Some(ScalperPattern::BulkPurchase {
                max_quantity: r.max_qty,
                limit: config.bulk_single_item_limit,
                printing_id: r.printing_id,
            }));
        }
    }
    Ok(None)
}

#[derive(sqlx::FromRow)]
struct SweepRow {
    printing_id: String,
    total_qty: i64,
}

async fn check_sweep(
    db: &PgPool,
    workspace_id: Uuid,
    buyer_hash: &str,
    transaction_id: Uuid,
    config: &DetectionConfig,
) -> Result<Vec<ScalperPattern>> {
    // For each printing in this transaction, check the buyer's all-time total.
    let rows: Vec<SweepRow> = sqlx::query_as(
        r#"
        SELECT tl.printing_id, SUM(tl.quantity)::bigint AS total_qty
        FROM transaction_lines tl
        JOIN transactions t ON t.id = tl.transaction_id
        WHERE tl.printing_id IS NOT NULL
          AND t.workspace_id = $1
          AND t.buyer_hash   = $2
          AND tl.printing_id IN (
              SELECT printing_id FROM transaction_lines WHERE transaction_id = $3
              AND printing_id IS NOT NULL
          )
        GROUP BY tl.printing_id
        "#,
    )
    .bind(workspace_id)
    .bind(buyer_hash)
    .bind(transaction_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .filter(|r| r.total_qty > config.sweep_printing_limit as i64)
        .map(|r| ScalperPattern::Sweep {
            printing_id: r.printing_id,
            total_quantity: r.total_qty,
            limit: config.sweep_printing_limit,
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct PriorPurchase {
    occurred_at: DateTime<Utc>,
}

async fn check_repeat(
    db: &PgPool,
    workspace_id: Uuid,
    buyer_hash: &str,
    current_occurred_at: DateTime<Utc>,
    config: &DetectionConfig,
) -> Result<Option<ScalperPattern>> {
    let prior: Option<PriorPurchase> = sqlx::query_as(
        r#"
        SELECT occurred_at FROM transactions
        WHERE workspace_id = $1
          AND buyer_hash   = $2
          AND occurred_at  < $3
          AND occurred_at  > $3 - ($4 || ' minutes')::interval
        ORDER BY occurred_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(buyer_hash)
    .bind(current_occurred_at)
    .bind(config.repeat_window_minutes)
    .fetch_optional(db)
    .await?;

    Ok(prior.map(|p| ScalperPattern::Repeat {
        previous_purchase_at: p.occurred_at,
        window_minutes: config.repeat_window_minutes,
    }))
}

// ── Public detection entry point ──────────────────────────────────────────────

/// Score a transaction for scalper patterns. Raises RiskFlag(s) and dispatches
/// notifications for any detected patterns. No-op for:
///  - Transactions with no buyer_hash (cash/anonymous — never falsely flagged).
///  - Allowlisted buyers.
///  - Workspaces with detection disabled.
pub async fn score_transaction(
    state: &AppState,
    workspace_id: Uuid,
    transaction_id: Uuid,
) -> Result<Vec<flags::RiskFlag>> {
    let db = &state.pool;
    let redis = &state.redis;

    let config = get_or_create_config(db, workspace_id).await?;
    if !config.enabled {
        return Ok(vec![]);
    }

    let txn: Option<TransactionRow> = sqlx::query_as(
        "SELECT buyer_hash, occurred_at FROM transactions WHERE id=$1 AND workspace_id=$2",
    )
    .bind(transaction_id)
    .bind(workspace_id)
    .fetch_optional(db)
    .await?;

    let txn = match txn {
        Some(t) => t,
        None => return Ok(vec![]),
    };

    let buyer_hash = match &txn.buyer_hash {
        None => return Ok(vec![]),
        Some(h) => h.clone(),
    };

    if is_allowlisted(db, workspace_id, &buyer_hash).await? {
        return Ok(vec![]);
    }

    let mut patterns: Vec<ScalperPattern> = Vec::new();

    if let Some(p) = check_velocity(db, redis, workspace_id, &buyer_hash, txn.occurred_at, &config).await? {
        patterns.push(p);
    }
    if let Some(p) = check_bulk(db, transaction_id, &config).await? {
        patterns.push(p);
    }
    patterns.extend(check_sweep(db, workspace_id, &buyer_hash, transaction_id, &config).await?);
    if let Some(p) = check_repeat(db, workspace_id, &buyer_hash, txn.occurred_at, &config).await? {
        patterns.push(p);
    }

    if patterns.is_empty() {
        return Ok(vec![]);
    }

    patterns.sort_by(|a, b| b.severity().cmp(&a.severity()));
    let primary = &patterns[0];
    let severity = primary.severity();
    let trigger_info = primary.trigger_info();
    let evidence = serde_json::json!({
        "buyer_hash": buyer_hash,
        "transaction_id": transaction_id,
        "patterns": patterns,
    });

    // create_flag inserts the row and dispatches in-app + email notifications.
    let flag = create_flag(
        state,
        workspace_id,
        FlagKind::Scalper,
        severity,
        TargetType::Transaction,
        transaction_id,
        &trigger_info,
        evidence,
    )
    .await?;

    Ok(vec![flag])
}

#[derive(sqlx::FromRow)]
struct TransactionRow {
    buyer_hash: Option<String>,
    occurred_at: DateTime<Utc>,
}

// ── Routes ────────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/:wid/scalper-config",       get(get_config_handler).patch(update_config_handler))
        .route("/workspaces/:wid/buyer-allowlist",      get(list_allowlist_handler).post(add_allowlist_handler))
        .route("/workspaces/:wid/buyer-allowlist/:id",  delete(remove_allowlist_handler))
        .route("/workspaces/:wid/scalper-scan/:txn_id", post(scan_transaction_handler))
}

fn seller_only(access: &WorkspaceAccess) -> Result<()> {
    if matches!(access.kind, WorkspaceKind::Collector) {
        return Err(AppError::Forbidden("scalper settings are only available to seller workspaces".into()));
    }
    Ok(())
}

// GET /workspaces/:wid/scalper-config
async fn get_config_handler(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
) -> Result<Json<DetectionConfig>> {
    let access = workspace_access(&s.pool, wid, auth.id).await?;
    seller_only(&access)?;
    Ok(Json(get_or_create_config(&s.pool, wid).await?))
}

// PATCH /workspaces/:wid/scalper-config
async fn update_config_handler(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Json(body): Json<UpdateConfigRequest>,
) -> Result<Json<DetectionConfig>> {
    let access = workspace_access(&s.pool, wid, auth.id).await?;
    seller_only(&access)?;
    require_owner(&access)?;
    if let Some(h) = body.velocity_window_hours {
        if h < 1 || h > 720 { return Err(AppError::BadRequest("velocity_window_hours must be 1–720".into())); }
    }
    if let Some(t) = body.velocity_threshold {
        if t < 1 { return Err(AppError::BadRequest("velocity_threshold must be ≥ 1".into())); }
    }
    if let Some(b) = body.bulk_single_item_limit {
        if b < 1 { return Err(AppError::BadRequest("bulk_single_item_limit must be ≥ 1".into())); }
    }
    if let Some(sv) = body.sweep_printing_limit {
        if sv < 1 { return Err(AppError::BadRequest("sweep_printing_limit must be ≥ 1".into())); }
    }
    if let Some(r) = body.repeat_window_minutes {
        if r < 1 || r > 1440 { return Err(AppError::BadRequest("repeat_window_minutes must be 1–1440".into())); }
    }
    Ok(Json(update_config(&s.pool, wid, body).await?))
}

// GET /workspaces/:wid/buyer-allowlist
async fn list_allowlist_handler(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
) -> Result<Json<Vec<AllowlistEntry>>> {
    let access = workspace_access(&s.pool, wid, auth.id).await?;
    seller_only(&access)?;
    Ok(Json(list_allowlist(&s.pool, wid).await?))
}

// POST /workspaces/:wid/buyer-allowlist
async fn add_allowlist_handler(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Json(body): Json<AddAllowlistRequest>,
) -> Result<(StatusCode, Json<AllowlistEntry>)> {
    let access = workspace_access(&s.pool, wid, auth.id).await?;
    seller_only(&access)?;
    require_owner(&access)?;
    if body.buyer_hash.trim().is_empty() {
        return Err(AppError::BadRequest("buyer_hash must not be empty".into()));
    }
    let entry = add_to_allowlist(&s.pool, wid, &body.buyer_hash, body.notes.as_deref(), auth.id).await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

// DELETE /workspaces/:wid/buyer-allowlist/:id
async fn remove_allowlist_handler(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((wid, entry_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let access = workspace_access(&s.pool, wid, auth.id).await?;
    seller_only(&access)?;
    require_owner(&access)?;
    remove_from_allowlist(&s.pool, wid, entry_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// POST /workspaces/:wid/scalper-scan/:txn_id  (on-demand scan)
async fn scan_transaction_handler(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((wid, txn_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<flags::RiskFlag>>> {
    let access = workspace_access(&s.pool, wid, auth.id).await?;
    seller_only(&access)?;
    let result = score_transaction(&s, wid, txn_id).await?;
    Ok(Json(result))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::build_valuation_service, crypto::EncryptionKey, grading::GradingService, integrations::pokemontcg::PokemonTcgClient};
    use lettre::AsyncSmtpTransport;
    use sqlx::PgPool;
    use std::sync::Arc;

    fn test_state(pool: PgPool) -> AppState {
        AppState {
            mailer: AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous("localhost").build(),
            base_url: "http://localhost".to_string(),
            encryption_key: EncryptionKey::generate(),
            tcg_client: Arc::new(PokemonTcgClient::new(None)),
            grading: Arc::new(GradingService::new(pool.clone(), None, 3600)),
            valuation: build_valuation_service(pool.clone()),
            redis: None,
            pool,
        }
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    async fn seed_workspace(db: &PgPool) -> Uuid {
        sqlx::query_scalar("INSERT INTO workspaces (name, kind) VALUES ('WS', 'seller') RETURNING id")
            .fetch_one(db).await.unwrap()
    }

    async fn seed_user(db: &PgPool, _workspace_id: Uuid) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO users (email, name, password_hash) VALUES ($1, 'Test', 'x') RETURNING id",
        )
        .bind(format!("user-{}@test.io", Uuid::new_v4()))
        .fetch_one(db).await.unwrap()
    }

    async fn seed_transaction(db: &PgPool, workspace_id: Uuid, buyer_hash: Option<&str>) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO transactions (workspace_id, buyer_hash) VALUES ($1, $2) RETURNING id",
        )
        .bind(workspace_id)
        .bind(buyer_hash)
        .fetch_one(db).await.unwrap()
    }

    async fn seed_transaction_line(db: &PgPool, txn_id: Uuid, printing_id: Option<&str>, qty: i32) {
        sqlx::query(
            "INSERT INTO transaction_lines (transaction_id, printing_id, quantity) VALUES ($1, $2, $3)",
        )
        .bind(txn_id)
        .bind(printing_id)
        .bind(qty)
        .execute(db).await.unwrap();
    }

    // ── config tests ──────────────────────────────────────────────────────────

    #[ignore = "requires DATABASE_URL"]
    #[sqlx::test]
    async fn get_config_creates_defaults(db: PgPool) {
        let ws = seed_workspace(&db).await;
        let cfg = get_or_create_config(&db, ws).await.unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.velocity_threshold, 5);
        assert_eq!(cfg.bulk_single_item_limit, 3);
    }

    #[ignore = "requires DATABASE_URL"]
    #[sqlx::test]
    async fn get_config_idempotent(db: PgPool) {
        let ws = seed_workspace(&db).await;
        let a = get_or_create_config(&db, ws).await.unwrap();
        let b = get_or_create_config(&db, ws).await.unwrap();
        assert_eq!(a.id, b.id);
    }

    #[ignore = "requires DATABASE_URL"]
    #[sqlx::test]
    async fn update_config_partial(db: PgPool) {
        let ws = seed_workspace(&db).await;
        let updated = update_config(&db, ws, UpdateConfigRequest {
            enabled: None,
            velocity_threshold: Some(10),
            velocity_window_hours: None,
            bulk_single_item_limit: None,
            sweep_printing_limit: None,
            repeat_window_minutes: None,
        }).await.unwrap();
        assert_eq!(updated.velocity_threshold, 10);
        assert!(updated.enabled); // unchanged default
    }

    // ── allowlist tests ───────────────────────────────────────────────────────

    #[ignore = "requires DATABASE_URL"]
    #[sqlx::test]
    async fn allowlist_crud(db: PgPool) {
        let ws = seed_workspace(&db).await;
        let user = seed_user(&db, ws).await;
        let hash = "abc123hash";

        // Add
        let entry = add_to_allowlist(&db, ws, hash, Some("trusted shop"), user).await.unwrap();
        assert_eq!(entry.buyer_hash, hash);
        assert_eq!(entry.notes.as_deref(), Some("trusted shop"));

        // Listed
        let list = list_allowlist(&db, ws).await.unwrap();
        assert_eq!(list.len(), 1);

        // Is allowlisted
        assert!(is_allowlisted(&db, ws, hash).await.unwrap());
        assert!(!is_allowlisted(&db, ws, "other").await.unwrap());

        // Remove
        remove_from_allowlist(&db, ws, entry.id).await.unwrap();
        assert!(!is_allowlisted(&db, ws, hash).await.unwrap());
    }

    #[ignore = "requires DATABASE_URL"]
    #[sqlx::test]
    async fn allowlist_remove_nonexistent_returns_not_found(db: PgPool) {
        let ws = seed_workspace(&db).await;
        let result = remove_from_allowlist(&db, ws, Uuid::new_v4()).await;
        assert!(matches!(result, Err(AppError::NotFound)));
    }

    #[ignore = "requires DATABASE_URL"]
    #[sqlx::test]
    async fn allowlist_upsert_notes(db: PgPool) {
        let ws = seed_workspace(&db).await;
        let user = seed_user(&db, ws).await;
        add_to_allowlist(&db, ws, "h1", Some("old"), user).await.unwrap();
        let updated = add_to_allowlist(&db, ws, "h1", Some("new"), user).await.unwrap();
        assert_eq!(updated.notes.as_deref(), Some("new"));
        // Still only one entry.
        assert_eq!(list_allowlist(&db, ws).await.unwrap().len(), 1);
    }

    // ── detection engine tests ────────────────────────────────────────────────

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and Postgres"]
    async fn anon_transaction_never_flagged(db: PgPool) {
        let state = test_state(db.clone());
        let ws = seed_workspace(&db).await;
        let txn = seed_transaction(&db, ws, None).await;
        let flags = score_transaction(&state, ws, txn).await.unwrap();
        assert!(flags.is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and Postgres"]
    async fn allowlisted_buyer_never_flagged(db: PgPool) {
        let state = test_state(db.clone());
        let ws = seed_workspace(&db).await;
        let user = seed_user(&db, ws).await;
        let hash = "trusted_buyer";
        add_to_allowlist(&db, ws, hash, None, user).await.unwrap();

        for _ in 0..10 {
            let txn = seed_transaction(&db, ws, Some(hash)).await;
            let _ = score_transaction(&state, ws, txn).await.unwrap();
        }
        let flags = score_transaction(&state, ws,
            seed_transaction(&db, ws, Some(hash)).await).await.unwrap();
        assert!(flags.is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and Postgres"]
    async fn detection_disabled_skips(db: PgPool) {
        let state = test_state(db.clone());
        let ws = seed_workspace(&db).await;
        update_config(&db, ws, UpdateConfigRequest {
            enabled: Some(false),
            velocity_window_hours: None, velocity_threshold: None,
            bulk_single_item_limit: None, sweep_printing_limit: None,
            repeat_window_minutes: None,
        }).await.unwrap();

        for _ in 0..20 {
            let txn = seed_transaction(&db, ws, Some("buyer_x")).await;
            let flags = score_transaction(&state, ws, txn).await.unwrap();
            assert!(flags.is_empty());
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and Postgres"]
    async fn bulk_pattern_detected(db: PgPool) {
        let state = test_state(db.clone());
        let ws = seed_workspace(&db).await;
        let txn = seed_transaction(&db, ws, Some("buyer_bulk")).await;
        seed_transaction_line(&db, txn, Some("printing-001"), 5).await;

        let flags = score_transaction(&state, ws, txn).await.unwrap();
        assert!(!flags.is_empty());
        assert!(flags[0].title.contains("bulk"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and Postgres"]
    async fn bulk_within_limit_not_flagged(db: PgPool) {
        let state = test_state(db.clone());
        let ws = seed_workspace(&db).await;
        let txn = seed_transaction(&db, ws, Some("buyer_ok")).await;
        seed_transaction_line(&db, txn, Some("printing-002"), 2).await;
        let flags = score_transaction(&state, ws, txn).await.unwrap();
        assert!(flags.is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL and Postgres"]
    async fn sweep_pattern_detected(db: PgPool) {
        let state = test_state(db.clone());
        let ws = seed_workspace(&db).await;
        let buyer = "buyer_sweep";
        let printing = "printing-limited-001";

        for _ in 0..3 {
            let txn = seed_transaction(&db, ws, Some(buyer)).await;
            seed_transaction_line(&db, txn, Some(printing), 4).await;
            let _ = score_transaction(&state, ws, txn).await.unwrap();
        }

        let txn = seed_transaction(&db, ws, Some(buyer)).await;
        seed_transaction_line(&db, txn, Some(printing), 2).await;
        let flags = score_transaction(&state, ws, txn).await.unwrap();
        assert!(!flags.is_empty());
        assert!(flags[0].title.contains("sweep") || flags[0].title.contains("velocity") || flags[0].title.contains("repeat"));
    }

    #[test]
    fn pattern_severity_ordering() {
        // Velocity 3× threshold → Critical.
        let p = ScalperPattern::Velocity { count: 15, window_hours: 24, threshold: 5 };
        assert_eq!(p.severity(), FlagSeverity::Critical);

        // Just over threshold → Medium.
        let p = ScalperPattern::Velocity { count: 6, window_hours: 24, threshold: 5 };
        assert_eq!(p.severity(), FlagSeverity::Medium);

        // Bulk 3× limit → Critical.
        let p = ScalperPattern::BulkPurchase { max_quantity: 9, limit: 3, printing_id: None };
        assert_eq!(p.severity(), FlagSeverity::Critical);

        // Repeat always Low.
        let p = ScalperPattern::Repeat {
            previous_purchase_at: Utc::now(),
            window_minutes: 30,
        };
        assert_eq!(p.severity(), FlagSeverity::Low);
    }
}
