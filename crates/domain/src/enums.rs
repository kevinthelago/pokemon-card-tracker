use serde::{Deserialize, Serialize};
use std::fmt;

/// Workspace kind — determines which features are available.
/// Seller workspaces get POS sync, reconciliation, and fraud detection.
/// Collector workspaces get the catalogue + valuation surface only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Seller,
    Collector,
}

impl WorkspaceKind {
    pub fn is_seller(self) -> bool {
        matches!(self, Self::Seller)
    }
}

impl fmt::Display for WorkspaceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Seller => write!(f, "seller"),
            Self::Collector => write!(f, "collector"),
        }
    }
}

/// Role a user holds within a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceRole {
    Owner,
    Staff,
}

impl fmt::Display for WorkspaceRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner => write!(f, "owner"),
            Self::Staff => write!(f, "staff"),
        }
    }
}

/// Physical grading company that issued the cert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Grader {
    PSA,
    CGC,
    BGS,
}

impl fmt::Display for Grader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PSA => write!(f, "PSA"),
            Self::CGC => write!(f, "CGC"),
            Self::BGS => write!(f, "BGS"),
        }
    }
}

/// Result of verifying a graded card's cert against the grader's API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Unverified,
    Verified,
    /// Grader API returned data but it doesn't match the card on record.
    Mismatch,
}

impl fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unverified => write!(f, "unverified"),
            Self::Verified => write!(f, "verified"),
            Self::Mismatch => write!(f, "mismatch"),
        }
    }
}

/// Category of fraud risk surfaced by the fraud engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskKind {
    Stolen,
    Scalper,
    Counterfeit,
}

impl fmt::Display for RiskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stolen => write!(f, "stolen"),
            Self::Scalper => write!(f, "scalper"),
            Self::Counterfeit => write!(f, "counterfeit"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for RiskSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskStatus {
    Open,
    Reviewed,
    Dismissed,
}

impl fmt::Display for RiskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Reviewed => write!(f, "reviewed"),
            Self::Dismissed => write!(f, "dismissed"),
        }
    }
}

/// The entity a RiskFlag targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTargetType {
    Instance,
    Inventory,
    Transaction,
}

/// Moderation lifecycle of a stolen-card report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Pending,
    Confirmed,
    Disputed,
    Rejected,
}

impl fmt::Display for ReportStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Confirmed => write!(f, "confirmed"),
            Self::Disputed => write!(f, "disputed"),
            Self::Rejected => write!(f, "rejected"),
        }
    }
}

/// Supported POS / channel providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PosProvider {
    Square,
    Shopify,
    Clover,
    Csv,
}

impl fmt::Display for PosProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Square => write!(f, "square"),
            Self::Shopify => write!(f, "shopify"),
            Self::Clover => write!(f, "clover"),
            Self::Csv => write!(f, "csv"),
        }
    }
}

/// Standard TCG card conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CardCondition {
    Mint,
    NearMint,
    LightlyPlayed,
    ModeratelyPlayed,
    HeavilyPlayed,
    Damaged,
}

impl fmt::Display for CardCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mint => write!(f, "MINT"),
            Self::NearMint => write!(f, "NEAR_MINT"),
            Self::LightlyPlayed => write!(f, "LIGHTLY_PLAYED"),
            Self::ModeratelyPlayed => write!(f, "MODERATELY_PLAYED"),
            Self::HeavilyPlayed => write!(f, "HEAVILY_PLAYED"),
            Self::Damaged => write!(f, "DAMAGED"),
        }
    }
}

/// Valuation price data sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationSource {
    Tcgplayer,
    Pricecharting,
    Cardmarket,
}
