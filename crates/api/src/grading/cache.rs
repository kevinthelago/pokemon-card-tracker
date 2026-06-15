//! Cache layer for grading verifications.
//!
//! Before calling the grader API, check the `grading_verifications` table for
//! a recent result. On a cache hit, return the stored outcome and skip the
//! external call (respects grader rate limits).
//!
//! A fresh result is persisted after every successful API call so future
//! lookups for the same cert are served from the DB.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

use super::adapter::VerifyOutcome;

#[derive(Debug, sqlx::FromRow)]
struct CachedRow {
    #[allow(dead_code)]
    id: Uuid,
    grader: String,
    cert_number: String,
    result_status: String,
    result_grade: Option<String>,
    result_card_name: Option<String>,
    result_set_name: Option<String>,
    result_year: Option<String>,
    raw_response: Option<serde_json::Value>,
}

impl CachedRow {
    fn into_outcome(self) -> VerifyOutcome {
        match self.result_status.as_str() {
            "verified" => VerifyOutcome::Verified {
                grader: self.grader,
                cert_number: self.cert_number,
                grade: self.result_grade.unwrap_or_default(),
                card_name: self.result_card_name.unwrap_or_default(),
                set_name: self.result_set_name,
                year: self.result_year,
                raw_response: self.raw_response,
            },
            "mismatch" => VerifyOutcome::Mismatch {
                grader: self.grader,
                cert_number: self.cert_number,
                returned_grade: self.result_grade.unwrap_or_default(),
                returned_card_name: self.result_card_name.unwrap_or_default(),
                raw_response: self.raw_response,
            },
            "not_found" => VerifyOutcome::NotFound {
                grader: self.grader,
                cert_number: self.cert_number,
            },
            _ => VerifyOutcome::Unavailable {
                grader: self.grader.clone(),
                cert_number: self.cert_number.clone(),
                reason: format!("unknown cached status: {}", self.result_status),
            },
        }
    }
}

/// Look up the most recent cached verification for (grader, cert_number).
/// Returns `None` when no unexpired record is found.
pub async fn get_cached(
    db: &PgPool,
    grader: &str,
    cert_number: &str,
    cache_ttl_secs: u64,
) -> Result<Option<VerifyOutcome>, AppError> {
    let ttl_interval = format!("{} seconds", cache_ttl_secs);

    let row = sqlx::query_as::<_, CachedRow>(
        "SELECT id, grader, cert_number, result_status, result_grade,
                result_card_name, result_set_name, result_year, raw_response
         FROM grading_verifications
         WHERE grader = $1
           AND cert_number = $2
           AND verified_at > NOW() - $3::INTERVAL
           -- Don't serve cached UNAVAILABLE — always retry the API.
           AND result_status != 'unavailable'
         ORDER BY verified_at DESC
         LIMIT 1",
    )
    .bind(grader)
    .bind(cert_number)
    .bind(ttl_interval)
    .fetch_optional(db)
    .await
    .map_err(AppError::Sqlx)?;

    Ok(row.map(|r| r.into_outcome()))
}

/// Persist a verification outcome to the `grading_verifications` table.
/// `card_instance_id` is optional — standalone verifications (not tied to a
/// specific instance) pass `None`.
pub async fn store_result(
    db: &PgPool,
    card_instance_id: Option<Uuid>,
    outcome: &VerifyOutcome,
) -> Result<Uuid, AppError> {
    let (grader, cert_number) = match outcome {
        VerifyOutcome::Verified {
            grader,
            cert_number,
            ..
        } => (grader, cert_number),
        VerifyOutcome::Mismatch {
            grader,
            cert_number,
            ..
        } => (grader, cert_number),
        VerifyOutcome::NotFound {
            grader,
            cert_number,
        } => (grader, cert_number),
        VerifyOutcome::Unavailable {
            grader,
            cert_number,
            ..
        } => (grader, cert_number),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO grading_verifications
            (card_instance_id, grader, cert_number, result_status,
             result_grade, result_card_name, result_set_name, result_year,
             raw_response, verified_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id",
    )
    .bind(card_instance_id)
    .bind(grader)
    .bind(cert_number)
    .bind(outcome.result_status())
    .bind(outcome.grade())
    .bind(outcome.card_name())
    .bind(outcome.set_name())
    .bind(outcome.year())
    .bind(outcome.raw_response())
    .bind(Utc::now())
    .fetch_one(db)
    .await
    .map_err(AppError::Sqlx)?;

    Ok(id)
}

/// Update `card_instances.verification_status` after a successful verification.
/// Skips the update for UNAVAILABLE outcomes (None instance_status).
pub async fn update_instance_status(
    db: &PgPool,
    instance_id: Uuid,
    outcome: &VerifyOutcome,
) -> Result<(), AppError> {
    let Some(new_status) = outcome.instance_status() else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE card_instances
         SET verification_status = $1, updated_at = NOW()
         WHERE id = $2",
    )
    .bind(new_status)
    .bind(instance_id)
    .execute(db)
    .await
    .map_err(AppError::Sqlx)?;

    Ok(())
}

/// Raise a counterfeit risk flag when a cert comes back NOT_FOUND.
/// Inserts into `risk_flags`; logs and swallows errors so the flag failure
/// never blocks the verification response.
pub async fn raise_counterfeit_flag(
    db: &PgPool,
    workspace_id: Option<Uuid>,
    grader: &str,
    cert_number: &str,
    card_instance_id: Option<Uuid>,
) {
    let subject_id = card_instance_id.unwrap_or_else(Uuid::new_v4);
    let detail = serde_json::json!({
        "grader": grader,
        "cert_number": cert_number,
        "reason": "cert not found in grader database"
    });

    let result = sqlx::query(
        "INSERT INTO risk_flags
            (workspace_id, kind, severity, status, subject_id, subject_type, detail)
         VALUES ($1, 'counterfeit', 'high', 'open', $2, 'card_instance', $3)",
    )
    .bind(workspace_id)
    .bind(subject_id)
    .bind(detail)
    .execute(db)
    .await;

    if let Err(e) = result {
        tracing::error!(
            grader,
            cert_number,
            error = %e,
            "failed to raise counterfeit flag"
        );
    }
}
