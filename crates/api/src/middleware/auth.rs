use crate::error::ApiError;
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Claims encoded in every JWT access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the authenticated user's UUID.
    pub sub: Uuid,
    /// Active workspace UUID (set after workspace selection).
    pub workspace_id: Option<Uuid>,
    /// Expiry (Unix timestamp).
    pub exp: u64,
    /// Issued at (Unix timestamp).
    pub iat: u64,
}

/// Axum extractor that validates the `Authorization: Bearer <token>` header
/// and returns the decoded [`Claims`].
pub struct AuthUser(pub Claims);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    crate::app::AppState: axum::extract::FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        use axum::extract::FromRef;
        let app_state = crate::app::AppState::from_ref(state);

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;

        let secret = app_state.config.auth.jwt_secret.as_bytes();
        let key = DecodingKey::from_secret(secret);
        let token_data = decode::<Claims>(token, &key, &Validation::default())
            .map_err(|_| ApiError::Unauthorized)?;

        Ok(AuthUser(token_data.claims))
    }
}

/// Issue a signed JWT access token.
pub fn sign_access_token(
    user_id: Uuid,
    workspace_id: Option<Uuid>,
    secret: &str,
    ttl_secs: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        sub: user_id,
        workspace_id,
        exp: now + ttl_secs,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}
