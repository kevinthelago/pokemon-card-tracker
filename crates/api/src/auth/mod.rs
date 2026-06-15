use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{app::AppState, error::AppError};

/// Authenticated user extracted from the `Authorization: Bearer <token>` header.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
}

#[derive(FromRow)]
struct AuthRow {
    id: Uuid,
    email: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);

        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;

        let row = sqlx::query_as::<_, AuthRow>(
            r#"
            SELECT u.id, u.email
            FROM sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token = $1 AND s.expires_at > NOW()
            "#,
        )
        .bind(token)
        .fetch_optional(&app.pool)
        .await
        .map_err(AppError::from)?
        .ok_or(AppError::Unauthorized)?;

        Ok(AuthUser { id: row.id, email: row.email })
    }
}

/// Optional auth: returns `None` for unauthenticated requests instead of 401.
pub struct OptionalAuthUser(pub Option<AuthUser>);

#[async_trait]
impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(OptionalAuthUser(
            AuthUser::from_request_parts(parts, state).await.ok(),
        ))
    }
}

pub fn build_mailer() -> anyhow::Result<AsyncSmtpTransport<Tokio1Executor>> {
    use anyhow::Context;
    use lettre::transport::smtp::authentication::Credentials;

    let host = std::env::var("SMTP_HOST").context("SMTP_HOST must be set")?;
    let port: u16 = std::env::var("SMTP_PORT")
        .unwrap_or_else(|_| "587".into())
        .parse()
        .context("SMTP_PORT must be a number")?;
    let username = std::env::var("SMTP_USERNAME").unwrap_or_default();
    let password = std::env::var("SMTP_PASSWORD").unwrap_or_default();

    let mut builder =
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)?.port(port);

    if !username.is_empty() {
        builder = builder.credentials(Credentials::new(username, password));
    }

    Ok(builder.build())
}
