/// Webhook signature verification helpers for POS providers.
///
/// Each provider uses a different signing scheme; this module provides
/// a common interface with provider-specific implementations.
use crate::error::ApiError;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify a Square webhook signature.
///
/// Square signs with HMAC-SHA256 of `{notification_url}{raw_body}` using the
/// webhook signature key as the secret.
pub fn verify_square(
    signature: &str,
    notification_url: &str,
    body: &[u8],
    secret: &str,
) -> Result<(), ApiError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::BadRequest("invalid webhook secret".into()))?;
    mac.update(notification_url.as_bytes());
    mac.update(body);
    let expected = B64.encode(mac.finalize().into_bytes());

    if !constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        return Err(ApiError::Unauthorized);
    }
    Ok(())
}

/// Verify a Shopify HMAC-SHA256 signature (provided in `X-Shopify-Hmac-SHA256`).
pub fn verify_shopify(signature_b64: &str, body: &[u8], secret: &str) -> Result<(), ApiError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| ApiError::BadRequest("invalid webhook secret".into()))?;
    mac.update(body);
    let expected_bytes = mac.finalize().into_bytes();

    let provided_bytes = B64
        .decode(signature_b64)
        .map_err(|_| ApiError::Unauthorized)?;

    if !constant_time_eq(&expected_bytes, &provided_bytes) {
        return Err(ApiError::Unauthorized);
    }
    Ok(())
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
