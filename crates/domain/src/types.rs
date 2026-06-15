use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Enums ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Seller,
    Collector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    Owner,
    Staff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Unverified,
    Failed,
}

impl VerificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Raw,
    Sealed,
    Graded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeKind {
    Cert,
    Upc,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    Open,
    Resolved,
    Dismissed,
}

// ── Core domain types ──────────────────────────────────────────────────────

/// A specific printing of a Pokémon card (set + number + variant + language).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printing {
    pub id: Uuid,
    pub tcg_api_id: String,
    pub name: String,
    pub set_id: String,
    pub set_name: String,
    pub number: String,
    pub variant: Option<String>,
    pub language: String,
    pub edition: Option<String>,
    pub image_url: Option<String>,
    pub image_url_large: Option<String>,
    pub supertype: Option<String>,
    pub rarity: Option<String>,
    pub cached_at: DateTime<Utc>,
}

/// A sealed product (booster box, ETB, etc.) identified by UPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedProduct {
    pub id: Uuid,
    pub upc: String,
    pub name: String,
    pub set_id: Option<String>,
    pub product_type: String,
    pub cached_at: DateTime<Utc>,
}

/// Raw or sealed inventory line — quantity-tracked.
/// Unique by (workspace_id, printing_id, condition).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: Option<Uuid>,
    pub sealed_product_id: Option<Uuid>,
    pub condition: Option<String>,
    pub quantity: i32,
    pub acquisition_cost_cents: Option<i32>,
    pub notes: Option<String>,
    pub photos: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single graded card instance — unique by (workspace_id, grader, cert_number).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardInstance {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: Option<Uuid>,
    pub grader: String,
    pub cert_number: String,
    pub grade: Option<String>,
    pub verification_status: VerificationStatus,
    pub acquisition_cost_cents: Option<i32>,
    pub notes: Option<String>,
    pub photos: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Grading verification result from an external grading service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingVerification {
    pub grader: String,
    pub cert_number: String,
    pub grade: Option<String>,
    pub card_name: Option<String>,
    pub set_name: Option<String>,
    pub year: Option<String>,
    pub status: VerificationStatus,
    pub raw_response: Option<serde_json::Value>,
}

/// Result from resolving a scanned barcode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResolution {
    pub barcode: String,
    pub kind: BarcodeKind,
    pub printing: Option<Printing>,
    pub sealed_product: Option<SealedProduct>,
    pub grading_verification: Option<GradingVerification>,
    pub needs_manual_entry: bool,
    pub error: Option<String>,
}

// ── Workspace / User (stubs for foundation stream) ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub kind: WorkspaceKind,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: MembershipRole,
    pub joined_at: DateTime<Utc>,
}
