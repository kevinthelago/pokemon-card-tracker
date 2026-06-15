//! Axum route handlers for grading verification.
//!
//! POST /grading/verify   — standalone verify (no card instance required)
//! POST /grading/verify/:instance_id — verify and update a specific CardInstance

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{app::AppState, error::AppError};

use super::VerifyOutcome;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/grading/verify", post(verify_standalone))
        .route("/grading/verify/:instance_id", post(verify_instance))
}

// ── Request / response shapes ──────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub grader: String,
    pub cert_number: String,
    /// Optional: when provided, identity-match is performed against this
    /// printing's card name, set, and number.
    pub claimed_card_name: Option<String>,
    /// Reserved for future identity matching against set and card number.
    pub claimed_set_name: Option<String>,
    pub claimed_number: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    /// One of: "verified", "mismatch", "not_found", "unavailable"
    pub status: &'static str,
    pub grader: String,
    pub cert_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    /// Populated when status is "mismatch" — describes what the grader returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch_detail: Option<MismatchDetail>,
    /// Populated when status is "not_found" — always true.
    pub counterfeit_flagged: bool,
    /// Populated when status is "unavailable".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// Whether this result was served from cache.
    pub cached: bool,
}

#[derive(Debug, Serialize)]
pub struct MismatchDetail {
    pub returned_grade: String,
    pub returned_card_name: String,
}

impl VerifyResponse {
    fn from_outcome(outcome: &VerifyOutcome, cached: bool) -> Self {
        match outcome {
            VerifyOutcome::Verified {
                grader,
                cert_number,
                grade,
                card_name,
                set_name,
                year,
                ..
            } => Self {
                status: "verified",
                grader: grader.clone(),
                cert_number: cert_number.clone(),
                grade: Some(grade.clone()),
                card_name: Some(card_name.clone()),
                set_name: set_name.clone(),
                year: year.clone(),
                mismatch_detail: None,
                counterfeit_flagged: false,
                unavailable_reason: None,
                cached,
            },
            VerifyOutcome::Mismatch {
                grader,
                cert_number,
                returned_grade,
                returned_card_name,
                ..
            } => Self {
                status: "mismatch",
                grader: grader.clone(),
                cert_number: cert_number.clone(),
                grade: None,
                card_name: None,
                set_name: None,
                year: None,
                mismatch_detail: Some(MismatchDetail {
                    returned_grade: returned_grade.clone(),
                    returned_card_name: returned_card_name.clone(),
                }),
                counterfeit_flagged: false,
                unavailable_reason: None,
                cached,
            },
            VerifyOutcome::NotFound { grader, cert_number } => Self {
                status: "not_found",
                grader: grader.clone(),
                cert_number: cert_number.clone(),
                grade: None,
                card_name: None,
                set_name: None,
                year: None,
                mismatch_detail: None,
                counterfeit_flagged: true,
                unavailable_reason: None,
                cached,
            },
            VerifyOutcome::Unavailable {
                grader,
                cert_number,
                reason,
            } => Self {
                status: "unavailable",
                grader: grader.clone(),
                cert_number: cert_number.clone(),
                grade: None,
                card_name: None,
                set_name: None,
                year: None,
                mismatch_detail: None,
                counterfeit_flagged: false,
                unavailable_reason: Some(reason.clone()),
                cached,
            },
        }
    }
}

// ── POST /grading/verify ───────────────────────────────────────────────────
//
// Standalone verify: no card instance in scope. Useful for "I'm about to buy
// this slab — is the cert real?" without cataloguing anything.

async fn verify_standalone(
    State(state): State<AppState>,
    Json(body): Json<VerifyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let grader_upper = body.grader.to_uppercase();

    validate_grader(&grader_upper)?;
    validate_cert_number(&body.cert_number)?;

    let (outcome, cached) = state
        .grading
        .verify_cert(&grader_upper, &body.cert_number, None)
        .await?;

    // For NOT_FOUND, raise a counterfeit flag without a workspace or instance.
    if outcome.is_not_found() {
        super::cache::raise_counterfeit_flag(
            &state.pool,
            None,
            &grader_upper,
            &body.cert_number,
            None,
        )
        .await;
    }

    let mut response = VerifyResponse::from_outcome(&outcome, cached);

    // Identity match: compare returned card name against claimed identity.
    if outcome.result_status() == "verified" {
        if let Some(claimed_name) = &body.claimed_card_name {
            if let Some(returned_name) = outcome.card_name() {
                if !names_match(claimed_name, returned_name) {
                    response.status = "mismatch";
                    response.mismatch_detail = Some(MismatchDetail {
                        returned_grade: outcome.grade().unwrap_or("").to_string(),
                        returned_card_name: returned_name.to_string(),
                    });
                }
            }
        }
    }

    let http_status = match response.status {
        "not_found" => StatusCode::OK,
        "unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::OK,
    };

    Ok((http_status, Json(response)))
}

// ── POST /grading/verify/:instance_id ─────────────────────────────────────
//
// Inline verify: looks up the CardInstance, runs the cert lookup, updates
// the instance's verification_status, and returns the outcome.

async fn verify_instance(
    State(state): State<AppState>,
    Path(instance_id): Path<Uuid>,
    Json(body): Json<VerifyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let grader_upper = body.grader.to_uppercase();

    validate_grader(&grader_upper)?;
    validate_cert_number(&body.cert_number)?;

    // Load the instance to get the workspace context for flag creation.
    let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM card_instances WHERE id = $1",
    )
    .bind(instance_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::Sqlx)?;

    if workspace_id.is_none() {
        return Err(AppError::NotFound);
    }

    let (outcome, cached) = state
        .grading
        .verify_cert(&grader_upper, &body.cert_number, Some(instance_id))
        .await?;

    // Update the instance's verification status (unless UNAVAILABLE).
    super::cache::update_instance_status(&state.pool, instance_id, &outcome).await?;

    // Raise flags based on outcome.
    if outcome.is_not_found() {
        super::cache::raise_counterfeit_flag(
            &state.pool,
            workspace_id,
            &grader_upper,
            &body.cert_number,
            Some(instance_id),
        )
        .await;
    }

    let mut response = VerifyResponse::from_outcome(&outcome, cached);

    // Identity match against the claimed printing.
    if outcome.result_status() == "verified" {
        if let Some(claimed_name) = &body.claimed_card_name {
            if let Some(returned_name) = outcome.card_name() {
                if !names_match(claimed_name, returned_name) {
                    response.status = "mismatch";
                    response.mismatch_detail = Some(MismatchDetail {
                        returned_grade: outcome.grade().unwrap_or("").to_string(),
                        returned_card_name: returned_name.to_string(),
                    });
                    // Update instance to mismatch.
                    let _ = sqlx::query(
                        "UPDATE card_instances SET verification_status = 'mismatch', updated_at = NOW() WHERE id = $1",
                    )
                    .bind(instance_id)
                    .execute(&state.pool)
                    .await;
                }
            }
        }
    }

    let http_status = match response.status {
        "unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::OK,
    };

    Ok((http_status, Json(response)))
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn validate_grader(grader: &str) -> Result<(), AppError> {
    match grader {
        "PSA" | "CGC" | "BGS" => Ok(()),
        other => Err(AppError::BadRequest(format!(
            "unsupported grader: '{other}' — supported: PSA, CGC, BGS"
        ))),
    }
}

fn validate_cert_number(cert: &str) -> Result<(), AppError> {
    if cert.trim().is_empty() {
        return Err(AppError::BadRequest("cert_number must not be empty".into()));
    }
    if cert.len() > 64 {
        return Err(AppError::BadRequest(
            "cert_number exceeds maximum length of 64".into(),
        ));
    }
    Ok(())
}

/// Fuzzy card name comparison — replaces punctuation with spaces and compares
/// case-insensitively, so "Pikachu-EX" matches "Pikachu EX".
fn names_match(claimed: &str, returned: &str) -> bool {
    let normalize = |s: &str| {
        s.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    normalize(claimed) == normalize(returned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_case_insensitive() {
        assert!(names_match("Charizard", "charizard"));
    }

    #[test]
    fn names_match_strips_punctuation() {
        assert!(names_match("Pikachu-EX", "Pikachu EX"));
    }

    #[test]
    fn names_match_different_names() {
        assert!(!names_match("Charizard", "Blastoise"));
    }

    #[test]
    fn validate_grader_accepts_known() {
        assert!(validate_grader("PSA").is_ok());
        assert!(validate_grader("CGC").is_ok());
        assert!(validate_grader("BGS").is_ok());
    }

    #[test]
    fn validate_grader_rejects_unknown() {
        assert!(validate_grader("XYZ").is_err());
    }

    #[test]
    fn validate_cert_number_rejects_empty() {
        assert!(validate_cert_number("").is_err());
        assert!(validate_cert_number("   ").is_err());
    }

    #[test]
    fn validate_cert_number_rejects_too_long() {
        let long = "A".repeat(65);
        assert!(validate_cert_number(&long).is_err());
    }

    #[test]
    fn validate_cert_number_accepts_valid() {
        assert!(validate_cert_number("12345678").is_ok());
    }
}
