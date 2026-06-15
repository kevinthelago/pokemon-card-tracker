//! PSA Cert Verification adapter.
//!
//! API: GET https://api.psacard.com/publicapi/cert/GetByCertNumber/{certNumber}
//! Auth: `Authorization: bearer {api_key}`
//!
//! PSA returns HTTP 200 with `PSACert: null` when a cert does not exist,
//! HTTP 401/403 when the key is invalid, and HTTP 429 when rate-limited.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use super::adapter::{GradingAdapter, VerifyOutcome};

const PSA_BASE_URL: &str = "https://api.psacard.com/publicapi/cert";

pub struct PsaAdapter {
    client: Client,
    api_key: String,
}

impl PsaAdapter {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build reqwest client"),
            api_key,
        }
    }
}

// ── PSA API response shapes ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PsaResponse {
    #[serde(rename = "PSACert")]
    psa_cert: Option<PsaCert>,
}

// Fields present in the response that aren't used directly — kept for JSON
// completeness (they're captured in raw_response for audit purposes).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PsaCert {
    cert_number: String,
    year: Option<String>,
    brand: Option<String>,
    subject: Option<String>,
    grade: Option<String>,
    variety: Option<String>,
    #[serde(rename = "GradeDescription")]
    grade_description: Option<String>,
    #[serde(rename = "CardNumber")]
    card_number: Option<String>,
}

// ── Adapter implementation ────────────────────────────────────────────────

#[async_trait]
impl GradingAdapter for PsaAdapter {
    fn grader_name(&self) -> &'static str {
        "PSA"
    }

    async fn verify(&self, cert_number: &str) -> VerifyOutcome {
        let url = format!("{}/GetByCertNumber/{}", PSA_BASE_URL, cert_number);

        let response = match self
            .client
            .get(&url)
            .header("Authorization", format!("bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return VerifyOutcome::Unavailable {
                    grader: "PSA".into(),
                    cert_number: cert_number.into(),
                    reason: format!("request failed: {e}"),
                };
            }
        };

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return VerifyOutcome::Unavailable {
                grader: "PSA".into(),
                cert_number: cert_number.into(),
                reason: "rate limited (HTTP 429)".into(),
            };
        }

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return VerifyOutcome::Unavailable {
                grader: "PSA".into(),
                cert_number: cert_number.into(),
                reason: format!("authentication failed (HTTP {})", status.as_u16()),
            };
        }

        if !status.is_success() {
            return VerifyOutcome::Unavailable {
                grader: "PSA".into(),
                cert_number: cert_number.into(),
                reason: format!("unexpected HTTP {}", status.as_u16()),
            };
        }

        let raw: serde_json::Value = match response.json().await {
            Ok(v) => v,
            Err(e) => {
                return VerifyOutcome::Unavailable {
                    grader: "PSA".into(),
                    cert_number: cert_number.into(),
                    reason: format!("failed to parse response: {e}"),
                };
            }
        };

        // Deserialize into typed struct for field extraction.
        let parsed: PsaResponse = match serde_json::from_value(raw.clone()) {
            Ok(p) => p,
            Err(e) => {
                return VerifyOutcome::Unavailable {
                    grader: "PSA".into(),
                    cert_number: cert_number.into(),
                    reason: format!("unexpected response shape: {e}"),
                };
            }
        };

        let Some(cert) = parsed.psa_cert else {
            // PSA returns PSACert: null when the cert is not in their database.
            return VerifyOutcome::NotFound {
                grader: "PSA".into(),
                cert_number: cert_number.into(),
            };
        };

        let grade = cert.grade.unwrap_or_else(|| "UNKNOWN".into());
        let card_name = cert.subject.unwrap_or_else(|| "Unknown".into());
        let set_name = cert.brand.clone();
        let year = cert.year.clone();

        VerifyOutcome::Verified {
            grader: "PSA".into(),
            cert_number: cert.cert_number,
            grade,
            card_name,
            set_name,
            year,
            raw_response: Some(raw),
        }
    }
}

// ── Stub adapters for unsupported graders ─────────────────────────────────

/// CGC adapter stub — plugs into the trait but always returns Unavailable.
/// Replace with a real implementation when CGC API access is available.
pub struct CgcAdapter;

#[async_trait]
impl GradingAdapter for CgcAdapter {
    fn grader_name(&self) -> &'static str {
        "CGC"
    }

    async fn verify(&self, cert_number: &str) -> VerifyOutcome {
        VerifyOutcome::Unavailable {
            grader: "CGC".into(),
            cert_number: cert_number.into(),
            reason: "CGC API integration not yet implemented".into(),
        }
    }
}

/// BGS adapter stub — same as CGC.
pub struct BgsAdapter;

#[async_trait]
impl GradingAdapter for BgsAdapter {
    fn grader_name(&self) -> &'static str {
        "BGS"
    }

    async fn verify(&self, cert_number: &str) -> VerifyOutcome {
        VerifyOutcome::Unavailable {
            grader: "BGS".into(),
            cert_number: cert_number.into(),
            reason: "BGS API integration not yet implemented".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psa_adapter_grader_name() {
        let adapter = PsaAdapter::new("test-key".into());
        assert_eq!(adapter.grader_name(), "PSA");
    }

    #[test]
    fn cgc_adapter_grader_name() {
        assert_eq!(CgcAdapter.grader_name(), "CGC");
    }

    #[test]
    fn bgs_adapter_grader_name() {
        assert_eq!(BgsAdapter.grader_name(), "BGS");
    }
}
