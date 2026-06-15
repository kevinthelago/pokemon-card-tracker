use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::grading::{Grader, VerificationStatus};

/// Card printing identity from the Pokémon TCG API.
/// This is shared reference data — not workspace-scoped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printing {
    pub id: String, // TCG API printing id (e.g. "base1-4")
    pub set_code: String,
    pub collector_number: String,
    pub name: String,
    pub rarity: String,
    pub variant: Option<String>,
    pub finish: Option<String>,
    pub language: String,
    pub edition: Option<String>,
    pub image_url: Option<String>,
}

/// Physical condition of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Condition {
    Mint,
    NearMint,
    LightlyPlayed,
    ModeratelyPlayed,
    HeavilyPlayed,
    Damaged,
}

impl Condition {
    pub fn from_str_flexible(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().replace([' ', '-', '_'], "").as_str() {
            "M" | "MINT" => Some(Self::Mint),
            "NM" | "NEARMINT" => Some(Self::NearMint),
            "LP" | "LIGHTLYPLAYED" | "EX" | "EXCELLENT" => Some(Self::LightlyPlayed),
            "MP" | "MODERATELYPLAYED" | "VG" | "VERYGOOD" | "GD" | "GOOD" => {
                Some(Self::ModeratelyPlayed)
            }
            "HP" | "HEAVILYPLAYED" | "PO" | "POOR" | "FR" | "FAIR" => {
                Some(Self::HeavilyPlayed)
            }
            "D" | "DAMAGED" => Some(Self::Damaged),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mint => "MINT",
            Self::NearMint => "NEAR_MINT",
            Self::LightlyPlayed => "LIGHTLY_PLAYED",
            Self::ModeratelyPlayed => "MODERATELY_PLAYED",
            Self::HeavilyPlayed => "HEAVILY_PLAYED",
            Self::Damaged => "DAMAGED",
        }
    }
}

impl std::fmt::Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Raw/sealed stock — quantity-tracked, no unique instance.
/// Unique key: (workspace_id, printing_id, condition).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: String,
    pub condition: Condition,
    pub quantity: i32,
    /// Acquisition cost in cents (e.g. 1000 = $10.00).
    pub acquisition_cost_cents: Option<i64>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A uniquely-tracked graded card.
/// Unique key: (grader, cert_number).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardInstance {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub printing_id: String,
    pub grader: Grader,
    pub cert_number: String,
    pub grade: String,
    pub verification_status: VerificationStatus,
    /// Acquisition cost in cents.
    pub acquisition_cost_cents: Option<i64>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Cached market value for a printing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Valuation {
    pub printing_id: String,
    pub source: ValuationSource,
    pub condition: Option<Condition>,
    pub price_cents: i64, // market price in cents
    pub currency: String,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationSource {
    Tcgplayer,
    Pricecharting,
}
