use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Provider enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Square,
    Shopify,
    Clover,
}

impl Provider {
    pub fn from_str_ci(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "square" => Some(Self::Square),
            "shopify" => Some(Self::Shopify),
            "clover" => Some(Self::Clover),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Shopify => "shopify",
            Self::Clover => "clover",
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// OAuth result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OAuthStartResult {
    pub authorization_url: String,
    pub state: String,
    pub pkce_verifier: String,
}

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in_secs: Option<u64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub merchant_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Data transfer types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PosProduct {
    pub sku: String,
    pub name: String,
    pub quantity: i64,
    pub price_cents: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AdapterSaleLine {
    pub sku: String,
    pub quantity: i64,
    pub price_cents: i64,
}

#[derive(Debug, Clone)]
pub struct AdapterSale {
    pub external_id: String,
    pub occurred_at: DateTime<Utc>,
    pub lines: Vec<AdapterSaleLine>,
}

// ---------------------------------------------------------------------------
// PosAdapter trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait PosAdapter: Send + Sync {
    fn provider(&self) -> Provider;

    fn oauth_start(&self, workspace_id: Uuid, redirect_uri: &str) -> OAuthStartResult;

    async fn oauth_exchange(
        &self,
        code: &str,
        pkce_verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, AppError>;

    async fn fetch_inventory(&self, access_token: &str) -> Result<Vec<PosProduct>, AppError>;

    async fn fetch_sales(
        &self,
        access_token: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<AdapterSale>, AppError>;

    async fn register_webhook(
        &self,
        access_token: &str,
        notification_url: &str,
    ) -> Result<Option<String>, AppError>;

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenSet, AppError>;

    async fn revoke_token(&self, access_token: &str) -> Result<(), AppError>;

    fn verify_webhook_signature(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        signing_key: &str,
    ) -> bool;
}

// ---------------------------------------------------------------------------
// PKCE helpers
// ---------------------------------------------------------------------------

pub fn generate_pkce_pair() -> (String, String) {
    use base64::Engine;
    use rand::Rng;
    use sha2::{Digest, Sha256};

    let verifier: String = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();

    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));

    (verifier, challenge)
}

pub fn generate_state() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

// ---------------------------------------------------------------------------
// Webhook HMAC helper (HMAC-SHA256 + base64, constant-time compare)
// ---------------------------------------------------------------------------

pub fn verify_hmac_sha256_base64(key: &[u8], message: &[u8], expected_base64: &str) -> bool {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = match Hmac::<Sha256>::new_from_slice(key) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(message);
    let result = mac.finalize().into_bytes();

    let expected = match base64::engine::general_purpose::STANDARD.decode(expected_base64) {
        Ok(b) => b,
        Err(_) => return false,
    };

    if result.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in result.iter().zip(expected.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_is_valid() {
        let (verifier, challenge) = generate_pkce_pair();
        assert_eq!(verifier.len(), 64);
        assert!(!challenge.is_empty());
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
        assert!(!challenge.contains('='));
    }

    #[test]
    fn generate_state_is_unique() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_eq!(s1.len(), 32);
        assert_ne!(s1, s2);
    }
}
