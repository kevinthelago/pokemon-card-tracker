use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::workspace::{MemberRole, WorkspaceKind},
};

pub const ACCESS_TOKEN_TTL_SECS: i64 = 15 * 60;

/// Claims embedded in the short-lived JWT access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — stable user UUID.
    pub sub: Uuid,
    pub email: String,
    pub workspace_id: Uuid,
    pub workspace_kind: WorkspaceKind,
    pub role: MemberRole,
    pub iat: i64,
    pub exp: i64,
}

pub fn encode_access_token(claims: &Claims, secret: &[u8]) -> Result<String, AppError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| AppError::Other(anyhow::anyhow!("JWT encode: {e}")))
}

pub fn decode_access_token(token: &str, secret: &[u8]) -> Result<Claims, AppError> {
    let mut v = Validation::default();
    v.leeway = 0;
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &v)
        .map(|d| d.claims)
        .map_err(|_| AppError::Unauthorized)
}
