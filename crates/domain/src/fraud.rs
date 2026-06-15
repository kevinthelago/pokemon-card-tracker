use crate::enums::{Grader, ReportStatus, RiskKind, RiskSeverity, RiskStatus, RiskTargetType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A fraud signal raised by the fraud engine against a workspace entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFlag {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: RiskKind,
    pub severity: RiskSeverity,
    pub target_type: RiskTargetType,
    pub target_id: Uuid,
    pub status: RiskStatus,
    /// Structured context (e.g. matched cert#, velocity window, buyer pattern).
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A community report of a stolen graded card.
/// Reports are moderated (see ReportStatus) before entering the confirmed stolen list.
/// The `(grader, cert_number)` pair drives stolen-cert matching in the fraud engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StolenReport {
    pub id: Uuid,
    pub grader: Grader,
    pub cert_number: String,
    pub reporter_user_id: Uuid,
    pub status: ReportStatus,
    /// Supporting evidence (photos, external links, notes).
    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A dispute filed against a StolenReport that the owner believes is erroneous.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeRecord {
    pub id: Uuid,
    pub stolen_report_id: Uuid,
    pub filed_by_user_id: Uuid,
    pub reason: String,
    pub evidence: serde_json::Value,
    pub resolution: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Cached result of verifying a cert number against a grader's public API.
/// Keyed by (grader, cert_number) — expires and refreshes on lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingVerification {
    pub id: Uuid,
    pub grader: Grader,
    pub cert_number: String,
    /// Full grader API response (identity, grade, authenticity flag, raw payload).
    pub result: serde_json::Value,
    pub verified_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Per-workspace configuration for the fraud detection engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scalper_enabled: bool,
    /// Time window (hours) for velocity-based scalper detection.
    pub scalper_velocity_window_hours: i32,
    /// Max purchase count within the window before flagging as scalper.
    pub scalper_velocity_threshold: i32,
    /// Minimum total spend within the window that also triggers a sweep flag.
    pub scalper_sweep_threshold: rust_decimal::Decimal,
    pub stolen_check_enabled: bool,
    pub config: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

/// A known-trusted buyer exempt from scalper detection for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyerAllowlistEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub buyer_hash: String,
    pub notes: Option<String>,
    pub created_by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
}
