use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "flag_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum FlagKind {
    StolenCard,
    Scalper,
    Counterfeit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "flag_severity", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum FlagSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "flag_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum FlagStatus {
    Open,
    Reviewed,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "target_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Card,
    Transaction,
    Buyer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "notification_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    NewRiskFlag,
    FlagEscalated,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub deep_link: Option<String>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Summary counts returned by GET /risk/summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskSummary {
    pub counts_by_kind: Vec<KindCount>,
    pub counts_by_severity: Vec<SeverityCount>,
    pub trend: Vec<TrendPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KindCount {
    pub kind: FlagKind,
    pub open: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SeverityCount {
    pub severity: FlagSeverity,
    pub open: i64,
    pub total: i64,
}

/// One data point in the open-vs-resolved trend (daily buckets, last 30 days).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrendPoint {
    pub day: DateTime<Utc>,
    pub opened: i64,
    pub resolved: i64,
}
