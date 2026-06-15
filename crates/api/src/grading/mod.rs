//! Grading verification module (verify-graded-card stream).
//!
//! Exposes a uniform adapter trait and a service that coordinates:
//!   1. Cache check (grading_verifications table)
//!   2. Adapter call (PSA / CGC / BGS)
//!   3. Result storage + CardInstance status update
//!
//! Owned paths: crates/api/src/grading/**

pub mod adapter;
pub mod cache;
pub mod psa;
pub mod routes;

pub use adapter::{GradingAdapter, VerifyOutcome};

use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppError;

use psa::{BgsAdapter, CgcAdapter, PsaAdapter};

/// Orchestrates cert verification across all graders.
pub struct GradingService {
    db: PgPool,
    psa: Arc<dyn GradingAdapter>,
    cgc: Arc<dyn GradingAdapter>,
    bgs: Arc<dyn GradingAdapter>,
    cache_ttl_secs: u64,
}

impl GradingService {
    pub fn new(db: PgPool, psa_api_key: Option<String>, cache_ttl_secs: u64) -> Self {
        let psa: Arc<dyn GradingAdapter> = match psa_api_key {
            Some(key) => Arc::new(PsaAdapter::new(key)),
            None => {
                tracing::warn!("PSA_API_KEY not set — PSA verifications will return UNAVAILABLE");
                Arc::new(PsaUnavailableAdapter)
            }
        };

        Self {
            db,
            psa,
            cgc: Arc::new(CgcAdapter),
            bgs: Arc::new(BgsAdapter),
            cache_ttl_secs,
        }
    }

    /// Look up a cert and return (outcome, was_cached).
    ///
    /// Flow:
    ///   1. Cache hit? → return cached outcome, was_cached = true
    ///   2. Call the appropriate adapter
    ///   3. Persist result to grading_verifications
    ///   4. Return fresh outcome, was_cached = false
    pub async fn verify_cert(
        &self,
        grader: &str,
        cert_number: &str,
        card_instance_id: Option<Uuid>,
    ) -> Result<(VerifyOutcome, bool), AppError> {
        // 1. Cache check.
        if let Some(cached) =
            cache::get_cached(&self.db, grader, cert_number, self.cache_ttl_secs).await?
        {
            tracing::debug!(grader, cert_number, "grading cache hit");
            return Ok((cached, true));
        }

        // 2. Adapter call.
        let adapter = self.adapter_for(grader);
        let outcome = adapter.verify(cert_number).await;

        // 3. Persist (swallow errors — don't fail verification because storage failed).
        if let Err(e) = cache::store_result(&self.db, card_instance_id, &outcome).await {
            tracing::error!(
                grader,
                cert_number,
                error = %e,
                "failed to store grading verification result"
            );
        }

        Ok((outcome, false))
    }

    fn adapter_for(&self, grader: &str) -> &dyn GradingAdapter {
        match grader {
            "PSA" => self.psa.as_ref(),
            "CGC" => self.cgc.as_ref(),
            "BGS" => self.bgs.as_ref(),
            _ => self.psa.as_ref(),
        }
    }
}

/// Fallback adapter used when PSA_API_KEY is not configured.
struct PsaUnavailableAdapter;

#[async_trait::async_trait]
impl GradingAdapter for PsaUnavailableAdapter {
    fn grader_name(&self) -> &'static str {
        "PSA"
    }

    async fn verify(&self, cert_number: &str) -> VerifyOutcome {
        VerifyOutcome::Unavailable {
            grader: "PSA".into(),
            cert_number: cert_number.into(),
            reason: "PSA_API_KEY not configured".into(),
        }
    }
}
