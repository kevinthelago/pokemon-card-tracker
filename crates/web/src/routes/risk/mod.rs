mod dashboard;

pub use dashboard::RiskDashboardPage;

// ─── Shared DTO types (mirror the API) ───────────────────────────────────────

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagKind {
    StolenCard,
    Scalper,
    Counterfeit,
}

impl FlagKind {
    pub fn label(&self) -> &'static str {
        match self {
            FlagKind::StolenCard => "Stolen card",
            FlagKind::Scalper => "Scalper",
            FlagKind::Counterfeit => "Counterfeit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl FlagSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            FlagSeverity::Low => "Low",
            FlagSeverity::Medium => "Medium",
            FlagSeverity::High => "High",
            FlagSeverity::Critical => "Critical",
        }
    }
    pub fn css_class(&self) -> &'static str {
        match self {
            FlagSeverity::Low => "severity--low",
            FlagSeverity::Medium => "severity--medium",
            FlagSeverity::High => "severity--high",
            FlagSeverity::Critical => "severity--critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlagStatus {
    Open,
    Reviewed,
    Dismissed,
}

impl FlagStatus {
    pub fn label(&self) -> &'static str {
        match self {
            FlagStatus::Open => "Open",
            FlagStatus::Reviewed => "Reviewed",
            FlagStatus::Dismissed => "Dismissed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Card,
    Transaction,
    Buyer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFlag {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: FlagKind,
    pub severity: FlagSeverity,
    pub status: FlagStatus,
    pub target_type: TargetType,
    pub target_id: Uuid,
    pub title: String,
    pub evidence: serde_json::Value,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub dismissed_by: Option<Uuid>,
    pub dismissed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagsPage {
    pub items: Vec<RiskFlag>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindCount {
    pub kind: FlagKind,
    pub open: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityCount {
    pub severity: FlagSeverity,
    pub open: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub day: DateTime<Utc>,
    pub opened: i64,
    pub resolved: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSummary {
    pub counts_by_kind: Vec<KindCount>,
    pub counts_by_severity: Vec<SeverityCount>,
    pub trend: Vec<TrendPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub deep_link: Option<String>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageBody {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkTriageBody {
    pub flag_ids: Vec<Uuid>,
    pub status: String,
}
