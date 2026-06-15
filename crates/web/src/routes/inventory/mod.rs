pub mod detail;
pub mod list;

pub use detail::InventoryDetailPage;
pub use list::InventoryListPage;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Shared types (mirror the API response types) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventoryItem {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryPage {
    pub items: Vec<InventoryItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryFilter {
    pub search: Option<String>,
    pub kind: Option<String>,
    pub set_code: Option<String>,
    pub rarity: Option<String>,
    pub condition: Option<String>,
    pub risk_flagged: bool,
    pub sort: String,
    pub order: String,
}

impl InventoryFilter {
    pub fn new() -> Self {
        Self {
            sort: "date".into(),
            order: "desc".into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchRequest {
    pub version: DateTime<Utc>,
    pub condition: Option<String>,
    pub quantity: Option<i64>,
    pub acquisition_cost: Option<Decimal>,
    pub notes: Option<String>,
    pub current_value: Option<Decimal>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BulkOp {
    Edit {
        ids: Vec<Uuid>,
        condition: Option<String>,
        acquisition_cost: Option<Decimal>,
    },
    Delete {
        ids: Vec<Uuid>,
    },
}
