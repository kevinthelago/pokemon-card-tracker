use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// The four outcomes of verifying a graded-card cert against a grader's API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum VerifyOutcome {
    /// Cert exists in the grader DB; identity returned by the grader.
    Verified {
        grader: String,
        cert_number: String,
        grade: String,
        card_name: String,
        set_name: Option<String>,
        year: Option<String>,
        raw_response: Option<serde_json::Value>,
    },
    /// Cert exists but the card identity differs from what was claimed —
    /// signals a swapped or re-holdered slab.
    Mismatch {
        grader: String,
        cert_number: String,
        returned_grade: String,
        returned_card_name: String,
        raw_response: Option<serde_json::Value>,
    },
    /// Cert does not exist in the grader's database — likely counterfeit or
    /// tampered slab. Must be flagged as a counterfeit signal.
    NotFound { grader: String, cert_number: String },
    /// Grader API is down or rate-limited. Do not block cataloguing; retry later.
    Unavailable {
        grader: String,
        cert_number: String,
        reason: String,
    },
}

impl VerifyOutcome {
    /// Status string stored in `grading_verifications.result_status`.
    pub fn result_status(&self) -> &'static str {
        match self {
            Self::Verified { .. } => "verified",
            Self::Mismatch { .. } => "mismatch",
            Self::NotFound { .. } => "not_found",
            Self::Unavailable { .. } => "unavailable",
        }
    }

    /// Maps to the `card_instances.verification_status` column.
    /// UNAVAILABLE does not update the instance status — returns None.
    pub fn instance_status(&self) -> Option<&'static str> {
        match self {
            Self::Verified { .. } => Some("verified"),
            Self::Mismatch { .. } => Some("mismatch"),
            // NOT_FOUND → mark as unverified; counterfeit flag raised separately.
            Self::NotFound { .. } => Some("unverified"),
            // UNAVAILABLE → don't touch instance status; leave as-is.
            Self::Unavailable { .. } => None,
        }
    }

    pub fn grade(&self) -> Option<&str> {
        match self {
            Self::Verified { grade, .. } => Some(grade.as_str()),
            Self::Mismatch { returned_grade, .. } => Some(returned_grade.as_str()),
            _ => None,
        }
    }

    pub fn card_name(&self) -> Option<&str> {
        match self {
            Self::Verified { card_name, .. } => Some(card_name.as_str()),
            Self::Mismatch {
                returned_card_name, ..
            } => Some(returned_card_name.as_str()),
            _ => None,
        }
    }

    pub fn set_name(&self) -> Option<&str> {
        match self {
            Self::Verified { set_name, .. } => set_name.as_deref(),
            _ => None,
        }
    }

    pub fn year(&self) -> Option<&str> {
        match self {
            Self::Verified { year, .. } => year.as_deref(),
            _ => None,
        }
    }

    pub fn raw_response(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Verified { raw_response, .. } => raw_response.as_ref(),
            Self::Mismatch { raw_response, .. } => raw_response.as_ref(),
            _ => None,
        }
    }

    /// True when the cert was not found — callers must raise a counterfeit flag.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }
}

/// Adapts a specific grading company's API to a uniform verify interface.
#[async_trait]
pub trait GradingAdapter: Send + Sync {
    /// Grader identifier returned with every outcome (e.g. "PSA", "CGC").
    fn grader_name(&self) -> &'static str;

    /// Look up a cert and return the outcome.
    /// Implementations must NOT block on 429 / 503 — return `Unavailable`
    /// with a reason string so callers can retry later.
    async fn verify(&self, cert_number: &str) -> VerifyOutcome;
}
