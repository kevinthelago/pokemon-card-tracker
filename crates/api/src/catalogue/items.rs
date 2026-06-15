use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;
use domain::{CardInstance, InventoryItem, VerificationStatus};

// ── Request types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AddRawItemRequest {
    pub workspace_id: Uuid,
    pub printing_id: Uuid,
    pub condition: String,
    #[serde(default = "one")]
    pub quantity: i32,
    pub acquisition_cost_cents: Option<i32>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub photos: Vec<String>,
}

fn one() -> i32 {
    1
}

#[derive(Debug, Deserialize)]
pub struct AddGradedItemRequest {
    pub workspace_id: Uuid,
    pub printing_id: Option<Uuid>,
    pub grader: String,
    pub cert_number: String,
    pub grade: Option<String>,
    pub verification_status: VerificationStatus,
    pub acquisition_cost_cents: Option<i32>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub photos: Vec<String>,
}

/// Response when a duplicate cert is found in the workspace.
#[derive(Debug, Serialize)]
pub struct DuplicateCertError {
    pub existing_instance_id: Uuid,
    pub grader: String,
    pub cert_number: String,
}

// ── DB row types ──────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct InventoryItemRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: Option<Uuid>,
    pub sealed_product_id: Option<Uuid>,
    pub condition: Option<String>,
    pub quantity: i32,
    pub acquisition_cost_cents: Option<i32>,
    pub notes: Option<String>,
    pub photos: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<InventoryItemRow> for InventoryItem {
    fn from(r: InventoryItemRow) -> Self {
        let photos = r
            .photos
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        InventoryItem {
            id: r.id,
            workspace_id: r.workspace_id,
            printing_id: r.printing_id,
            sealed_product_id: r.sealed_product_id,
            condition: r.condition,
            quantity: r.quantity,
            acquisition_cost_cents: r.acquisition_cost_cents,
            notes: r.notes,
            photos,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CardInstanceRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: Option<Uuid>,
    pub grader: String,
    pub cert_number: String,
    pub grade: Option<String>,
    pub verification_status: String,
    pub acquisition_cost_cents: Option<i32>,
    pub notes: Option<String>,
    pub photos: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<CardInstanceRow> for CardInstance {
    fn from(r: CardInstanceRow) -> Self {
        let photos = r
            .photos
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let verification_status = match r.verification_status.as_str() {
            "verified" => VerificationStatus::Verified,
            "failed" => VerificationStatus::Failed,
            _ => VerificationStatus::Unverified,
        };

        CardInstance {
            id: r.id,
            workspace_id: r.workspace_id,
            printing_id: r.printing_id,
            grader: r.grader,
            cert_number: r.cert_number,
            grade: r.grade,
            verification_status,
            acquisition_cost_cents: r.acquisition_cost_cents,
            notes: r.notes,
            photos,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// ── Service ────────────────────────────────────────────────────────────────

pub struct ItemsService {
    db: PgPool,
}

impl ItemsService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Upsert a raw or sealed inventory item.
    ///
    /// If a row already exists for (workspace_id, printing_id, condition) the
    /// quantity is incremented and the acquisition cost is updated when provided.
    pub async fn upsert_inventory_item(
        &self,
        req: &AddRawItemRequest,
    ) -> Result<InventoryItem, AppError> {
        let photos =
            serde_json::to_value(&req.photos).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;

        let row = sqlx::query_as::<_, InventoryItemRow>(
            "INSERT INTO inventory_items (
                workspace_id, printing_id, condition, quantity,
                acquisition_cost_cents, notes, photos
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (workspace_id, printing_id, condition) DO UPDATE SET
                quantity               = inventory_items.quantity + EXCLUDED.quantity,
                acquisition_cost_cents = COALESCE(EXCLUDED.acquisition_cost_cents,
                                                  inventory_items.acquisition_cost_cents),
                notes                  = COALESCE(EXCLUDED.notes, inventory_items.notes),
                updated_at             = NOW()
             RETURNING id, workspace_id, printing_id, sealed_product_id, condition,
                       quantity, acquisition_cost_cents, notes, photos, created_at, updated_at",
        )
        .bind(req.workspace_id)
        .bind(req.printing_id)
        .bind(&req.condition)
        .bind(req.quantity)
        .bind(req.acquisition_cost_cents)
        .bind(req.notes.as_deref())
        .bind(photos)
        .fetch_one(&self.db)
        .await
        .map_err(AppError::Sqlx)?;

        let item = InventoryItem::from(row);
        let item_id = item.id;
        enqueue_valuation(item_id, "inventory_item");

        Ok(item)
    }

    /// Create a graded card instance (unique per workspace + grader + cert).
    ///
    /// Returns `Err(AppError::Conflict)` if the cert already exists in this
    /// workspace; the error message embeds the existing instance id so the UI
    /// can surface a link to it.
    pub async fn add_card_instance(
        &self,
        req: &AddGradedItemRequest,
    ) -> Result<CardInstance, AppError> {
        // Check for duplicate cert before inserting so we can return a useful error.
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM card_instances
             WHERE workspace_id = $1 AND grader = $2 AND cert_number = $3",
        )
        .bind(req.workspace_id)
        .bind(&req.grader)
        .bind(&req.cert_number)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::Sqlx)?;

        if let Some((existing_id,)) = existing {
            return Err(AppError::Conflict(format!(
                "cert {}/{} already exists in this workspace (instance {})",
                req.grader, req.cert_number, existing_id
            )));
        }

        let photos =
            serde_json::to_value(&req.photos).map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;

        let row = sqlx::query_as::<_, CardInstanceRow>(
            "INSERT INTO card_instances (
                workspace_id, printing_id, grader, cert_number, grade,
                verification_status, acquisition_cost_cents, notes, photos
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING id, workspace_id, printing_id, grader, cert_number, grade,
                       verification_status, acquisition_cost_cents, notes, photos,
                       created_at, updated_at",
        )
        .bind(req.workspace_id)
        .bind(req.printing_id)
        .bind(&req.grader)
        .bind(&req.cert_number)
        .bind(req.grade.as_deref())
        .bind(req.verification_status.as_str())
        .bind(req.acquisition_cost_cents)
        .bind(req.notes.as_deref())
        .bind(photos)
        .fetch_one(&self.db)
        .await
        .map_err(AppError::Sqlx)?;

        let instance = CardInstance::from(row);
        let instance_id = instance.id;
        enqueue_valuation(instance_id, "card_instance");

        Ok(instance)
    }
}

// ── Background job shim ────────────────────────────────────────────────────

/// Enqueue a non-blocking valuation fetch.
///
/// The real implementation uses apalis (foundation/track-values streams).
/// This shim uses tokio::spawn so the endpoint can return immediately.
fn enqueue_valuation(item_id: Uuid, kind: &'static str) {
    tokio::spawn(async move {
        tracing::debug!("valuation enqueued: {} {}", kind, item_id);
        // TODO(track-values stream): replace with apalis storage push.
    });
}
