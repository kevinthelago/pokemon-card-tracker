use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Grading company.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Grader {
    Psa,
    Cgc,
    Bgs,
}

impl Grader {
    pub fn from_str_flexible(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PSA" => Some(Self::Psa),
            "CGC" => Some(Self::Cgc),
            "BGS" | "BECKETT" => Some(Self::Bgs),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Psa => "PSA",
            Self::Cgc => "CGC",
            Self::Bgs => "BGS",
        }
    }
}

impl std::fmt::Display for Grader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Verification status of a graded card against the grader's API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Unverified,
    Verified,
    Mismatch,
    Failed,
}

impl Default for VerificationStatus {
    fn default() -> Self {
        Self::Unverified
    }
}

/// Cached result of a grader cert lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingVerification {
    pub grader: Grader,
    pub cert_number: String,
    pub result: serde_json::Value,
    pub verified_at: DateTime<Utc>,
}
