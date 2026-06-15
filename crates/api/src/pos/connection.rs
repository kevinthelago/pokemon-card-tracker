use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    crypto::{decrypt_token, encrypt_token, EncryptionKey},
    error::AppError,
    pos::adapter::TokenSet,
};

// ---------------------------------------------------------------------------
// DB row — full schema with encrypted tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PosConnectionRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub provider: String,
    pub external_account_id: Option<String>,
    pub status: String,
    pub encrypted_access_token: Option<Vec<u8>>,
    pub encrypted_refresh_token: Option<Vec<u8>>,
    #[allow(dead_code)]
    pub token_nonce: Option<Vec<u8>>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub webhook_id: Option<String>,
    pub uses_polling: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub pkce_verifier: Option<String>,
    pub oauth_state: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn provider_display(provider: &str) -> &'static str {
    match provider {
        "square" => "Square",
        "shopify" => "Shopify POS",
        "clover" => "Clover",
        _ => "Unknown",
    }
}

// ---------------------------------------------------------------------------
// DTO returned to clients (no sensitive fields)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PosConnectionDto {
    pub id: Uuid,
    pub provider: String,
    pub provider_display: String,
    pub external_account_id: Option<String>,
    pub status: String,
    pub uses_polling: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

impl From<&PosConnectionRow> for PosConnectionDto {
    fn from(c: &PosConnectionRow) -> Self {
        Self {
            id: c.id,
            provider: c.provider.clone(),
            provider_display: provider_display(&c.provider).to_owned(),
            external_account_id: c.external_account_id.clone(),
            status: c.status.clone(),
            uses_polling: c.uses_polling,
            last_synced_at: c.last_synced_at,
            error_message: c.error_message.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ConnectionRepo {
    pool: PgPool,
    key: EncryptionKey,
}

impl ConnectionRepo {
    pub fn new(pool: PgPool, key: EncryptionKey) -> Self {
        Self { pool, key }
    }

    pub async fn create_pending(
        &self,
        workspace_id: Uuid,
        provider: &str,
        oauth_state: &str,
        pkce_verifier: &str,
    ) -> Result<PosConnectionRow, AppError> {
        sqlx::query_as(
            r#"
            INSERT INTO pos_connections
                (workspace_id, provider, oauth_state, pkce_verifier, status)
            VALUES ($1, $2, $3, $4, 'pending')
            RETURNING *
            "#,
        )
        .bind(workspace_id)
        .bind(provider)
        .bind(oauth_state)
        .bind(pkce_verifier)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn find_by_id(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Option<PosConnectionRow>, AppError> {
        sqlx::query_as("SELECT * FROM pos_connections WHERE id = $1 AND workspace_id = $2")
            .bind(id)
            .bind(workspace_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)
    }

    pub async fn find_by_state(
        &self,
        oauth_state: &str,
    ) -> Result<Option<PosConnectionRow>, AppError> {
        sqlx::query_as(
            "SELECT * FROM pos_connections WHERE oauth_state = $1 AND status = 'pending'",
        )
        .bind(oauth_state)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn list_active(&self, workspace_id: Uuid) -> Result<Vec<PosConnectionRow>, AppError> {
        sqlx::query_as(
            r#"
            SELECT * FROM pos_connections
            WHERE workspace_id = $1 AND status IN ('active', 'error', 'pending')
            ORDER BY created_at DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn activate(
        &self,
        id: Uuid,
        tokens: &TokenSet,
        webhook_id: Option<String>,
        uses_polling: bool,
    ) -> Result<PosConnectionRow, AppError> {
        let enc_access = encrypt_token(&self.key, &tokens.access_token)?;
        let enc_refresh = tokens
            .refresh_token
            .as_deref()
            .map(|rt| encrypt_token(&self.key, rt))
            .transpose()?;

        let expires_at = tokens.expires_at.or_else(|| {
            tokens
                .expires_in_secs
                .map(|s| Utc::now() + Duration::seconds(s as i64))
        });

        sqlx::query_as(
            r#"
            UPDATE pos_connections SET
                status                  = 'active',
                external_account_id     = $2,
                encrypted_access_token  = $3,
                encrypted_refresh_token = $4,
                token_expires_at        = $5,
                webhook_id              = $6,
                uses_polling            = $7,
                oauth_state             = NULL,
                pkce_verifier           = NULL,
                error_message           = NULL,
                updated_at              = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&tokens.merchant_id)
        .bind(enc_access)
        .bind(enc_refresh)
        .bind(expires_at)
        .bind(webhook_id)
        .bind(uses_polling)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn set_error(&self, id: Uuid, message: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE pos_connections
            SET status = 'error', error_message = $2, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(message)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(())
    }

    pub async fn disconnect(&self, id: Uuid, workspace_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE pos_connections SET
                status                  = 'disconnected',
                encrypted_access_token  = NULL,
                encrypted_refresh_token = NULL,
                token_nonce             = NULL,
                webhook_id              = NULL,
                error_message           = NULL,
                updated_at              = NOW()
            WHERE id = $1 AND workspace_id = $2
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(())
    }

    pub async fn update_last_synced(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE pos_connections SET last_synced_at = NOW(), updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(())
    }

    pub async fn find_expiring_tokens(
        &self,
        within: Duration,
    ) -> Result<Vec<PosConnectionRow>, AppError> {
        let deadline = Utc::now() + within;

        sqlx::query_as(
            r#"
            SELECT * FROM pos_connections
            WHERE status = 'active'
              AND token_expires_at IS NOT NULL
              AND token_expires_at <= $1
            "#,
        )
        .bind(deadline)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub async fn upsert_by_merchant(
        &self,
        workspace_id: Uuid,
        provider: &str,
        tokens: &TokenSet,
        webhook_id: Option<String>,
        uses_polling: bool,
    ) -> Result<PosConnectionRow, AppError> {
        let enc_access = encrypt_token(&self.key, &tokens.access_token)?;
        let enc_refresh = tokens
            .refresh_token
            .as_deref()
            .map(|rt| encrypt_token(&self.key, rt))
            .transpose()?;

        let expires_at = tokens.expires_at.or_else(|| {
            tokens
                .expires_in_secs
                .map(|s| Utc::now() + Duration::seconds(s as i64))
        });

        sqlx::query_as(
            r#"
            INSERT INTO pos_connections
                (workspace_id, provider, external_account_id, status,
                 encrypted_access_token, encrypted_refresh_token,
                 token_expires_at, webhook_id, uses_polling)
            VALUES ($1, $2, $3, 'active', $4, $5, $6, $7, $8)
            ON CONFLICT (workspace_id, provider, external_account_id)
            DO UPDATE SET
                status                  = 'active',
                encrypted_access_token  = EXCLUDED.encrypted_access_token,
                encrypted_refresh_token = EXCLUDED.encrypted_refresh_token,
                token_expires_at        = EXCLUDED.token_expires_at,
                webhook_id              = EXCLUDED.webhook_id,
                uses_polling            = EXCLUDED.uses_polling,
                error_message           = NULL,
                oauth_state             = NULL,
                pkce_verifier           = NULL,
                updated_at              = NOW()
            RETURNING *
            "#,
        )
        .bind(workspace_id)
        .bind(provider)
        .bind(&tokens.merchant_id)
        .bind(enc_access)
        .bind(enc_refresh)
        .bind(expires_at)
        .bind(webhook_id)
        .bind(uses_polling)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)
    }

    pub fn decrypt_access_token(&self, conn: &PosConnectionRow) -> Result<String, AppError> {
        let data = conn
            .encrypted_access_token
            .as_ref()
            .ok_or_else(|| AppError::Other(anyhow::anyhow!("No access token on connection")))?;
        decrypt_token(&self.key, data)
    }

    pub fn decrypt_refresh_token(
        &self,
        conn: &PosConnectionRow,
    ) -> Result<Option<String>, AppError> {
        match &conn.encrypted_refresh_token {
            None => Ok(None),
            Some(data) => Ok(Some(decrypt_token(&self.key, data)?)),
        }
    }
}
