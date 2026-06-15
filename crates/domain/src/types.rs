use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Verification status stored on card_instances ────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Verified,
    Unverified,
    /// Cert exists in grader DB but the identity returned doesn't match the
    /// card on record — signals a swapped or re-holdered slab.
    Mismatch,
}

impl VerificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Mismatch => "mismatch",
        }
    }
}

impl std::fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Card identity ───────────────────────────────────────────────────────────

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

// ── Graded card instance ────────────────────────────────────────────────────

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

// ── Grading verification result ─────────────────────────────────────────────

/// Persisted result of one grading-API verification lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingVerification {
    pub id: Uuid,
    pub card_instance_id: Option<Uuid>,
    pub grader: String,
    pub cert_number: String,
    /// One of: "verified", "mismatch", "not_found", "unavailable"
    pub result_status: String,
    pub result_grade: Option<String>,
    pub result_card_name: Option<String>,
    pub result_set_name: Option<String>,
    pub result_year: Option<String>,
    pub raw_response: Option<serde_json::Value>,
    pub verified_at: DateTime<Utc>,
}
