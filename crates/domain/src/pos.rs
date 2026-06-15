use crate::enums::PosProvider;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OAuth / API connection to a POS provider for a workspace.
/// Tokens are stored encrypted at the DB layer (column-level AES-256-GCM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosConnection {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub provider: PosProvider,
    /// Opaque at this layer — encrypted bytes are handled in the API crate.
    #[serde(skip_serializing)]
    pub access_token_enc: Option<Vec<u8>>,
    #[serde(skip_serializing)]
    pub refresh_token_enc: Option<Vec<u8>>,
    pub config: serde_json::Value,
    pub status: PosConnectionStatus,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosConnectionStatus {
    Active,
    Inactive,
    Error,
}

/// Maps an external POS product ID to a Printing or CardInstance in the catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosProductMapping {
    pub id: Uuid,
    pub pos_connection_id: Uuid,
    pub external_product_id: String,
    pub printing_id: Option<String>,
    pub card_instance_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// A sale event ingested from a POS provider.
/// `buyer_hash` is a salted SHA-256 of the buyer identifier for scalper analysis —
/// the raw PII is never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub pos_connection_id: Option<Uuid>,
    pub external_id: Option<String>,
    pub buyer_hash: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub total: Decimal,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A line within a Transaction — one item sold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLine {
    pub id: Uuid,
    pub transaction_id: Uuid,
    /// Points to either a Printing (raw) or a CardInstance (graded) — not both.
    pub printing_id: Option<String>,
    pub card_instance_id: Option<Uuid>,
    pub qty: i32,
    pub unit_price: Decimal,
    pub created_at: DateTime<Utc>,
}

/// A discrepancy detected between POS stock levels and the catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationDiscrepancy {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub pos_connection_id: Option<Uuid>,
    pub printing_id: Option<String>,
    pub card_instance_id: Option<Uuid>,
    pub expected_qty: Option<i32>,
    pub actual_qty: Option<i32>,
    pub discrepancy_kind: String,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
    pub detected_at: DateTime<Utc>,
}
