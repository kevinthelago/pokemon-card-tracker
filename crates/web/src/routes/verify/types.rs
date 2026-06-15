use serde::{Deserialize, Serialize};

/// Mirrors the API's VerifyResponse shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyResponse {
    /// One of: "verified", "mismatch", "not_found", "unavailable"
    pub status: String,
    pub grader: String,
    pub cert_number: String,
    pub grade: Option<String>,
    pub card_name: Option<String>,
    pub set_name: Option<String>,
    pub year: Option<String>,
    pub mismatch_detail: Option<MismatchDetail>,
    #[serde(default)]
    pub counterfeit_flagged: bool,
    pub unavailable_reason: Option<String>,
    #[serde(default)]
    pub cached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MismatchDetail {
    pub returned_grade: String,
    pub returned_card_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub grader: String,
    pub cert_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_card_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_set_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_number: Option<String>,
}

/// Which grader to verify against.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Grader {
    #[default]
    Psa,
    Cgc,
    Bgs,
}

impl Grader {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Psa => "PSA",
            Self::Cgc => "CGC",
            Self::Bgs => "BGS",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "CGC" => Self::Cgc,
            "BGS" => Self::Bgs,
            _ => Self::Psa,
        }
    }
}
