use crate::enums::{CardCondition, Grader, ValuationSource, VerificationStatus};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Reference identity for a specific Pokémon TCG card printing.
/// Sourced from pokemontcg.io — read-mostly, shared across workspaces.
/// This is "what the card is", not which physical copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printing {
    /// Pokémon TCG API identifier (e.g. "xy1-1").
    pub id: String,
    pub set_code: String,
    pub collector_number: String,
    pub name: String,
    pub rarity: Option<String>,
    pub variant: Option<String>,
    pub finish: Option<String>,
    pub language: String,
    pub edition: Option<String>,
    pub image_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Raw or sealed stock, tracked as a quantity of identical copies.
/// Identity key: (workspace_id, printing_id, condition) — no unique instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: String,
    pub condition: CardCondition,
    pub quantity: i32,
    pub acquisition_cost: Option<Decimal>,
    pub notes: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A uniquely-tracked graded card.
/// Identity key: (grader, cert_number) — globally unique, verifiable via grader API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardInstance {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: String,
    pub grader: Grader,
    pub cert_number: String,
    pub grade: Option<String>,
    pub verification_status: VerificationStatus,
    pub acquisition_cost: Option<Decimal>,
    /// Ordered list of photo URLs.
    pub photos: Vec<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Latest cached market price for a printing at an optional condition/grade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Valuation {
    pub id: Uuid,
    pub printing_id: String,
    /// None = base/any condition price.
    pub condition: Option<CardCondition>,
    /// None = ungraded; Some = grade-specific price (e.g. "PSA 10").
    pub grade: Option<String>,
    pub source: ValuationSource,
    pub price: Decimal,
    pub currency: String,
    pub fetched_at: DateTime<Utc>,
}

/// Historical price snapshot for charting value over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationSnapshot {
    pub id: Uuid,
    pub printing_id: String,
    pub condition: Option<CardCondition>,
    pub grade: Option<String>,
    pub source: ValuationSource,
    pub price: Decimal,
    pub currency: String,
    pub snapshotted_at: DateTime<Utc>,
}
