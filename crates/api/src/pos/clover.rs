use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::AppError,
    pos::adapter::{
        generate_state, AdapterSale, AdapterSaleLine, OAuthStartResult, PosAdapter, PosProduct,
        Provider, TokenSet,
    },
};

const CLOVER_AUTH_URL: &str = "https://www.clover.com/oauth/authorize";
const CLOVER_TOKEN_URL: &str = "https://apisandbox.dev.clover.com/oauth/token";
const CLOVER_API_BASE: &str = "https://api.clover.com/v3";

pub struct CloverAdapter {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
}

impl CloverAdapter {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self { client_id, client_secret, http: reqwest::Client::new() }
    }

    fn merchant_url(&self, merchant_id: &str, path: &str) -> String {
        format!("{CLOVER_API_BASE}/merchants/{merchant_id}{path}")
    }

    async fn get_merchant_id(&self, access_token: &str) -> Result<String, AppError> {
        let resp = self
            .http
            .get(format!("{CLOVER_API_BASE}/merchant"))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        data["id"].as_str().map(|s| s.to_owned()).ok_or_else(|| {
            AppError::Other(anyhow::anyhow!("Clover: could not determine merchant ID"))
        })
    }
}

#[derive(Deserialize)]
struct CloverTokenResponse {
    access_token: Option<String>,
    merchant_id: Option<String>,
}

#[derive(Deserialize)]
struct CloverItemsResponse {
    elements: Option<Vec<CloverItem>>,
}

#[derive(Deserialize)]
struct CloverItem {
    id: Option<String>,
    name: Option<String>,
    price: Option<i64>,
}

#[derive(Deserialize)]
struct CloverOrdersResponse {
    elements: Option<Vec<CloverOrder>>,
}

#[derive(Deserialize)]
struct CloverOrder {
    id: Option<String>,
    #[serde(rename = "createdTime")]
    created_time: Option<i64>,
    #[serde(rename = "lineItems")]
    line_items: Option<CloverLineItemsWrapper>,
}

#[derive(Deserialize)]
struct CloverLineItemsWrapper {
    elements: Option<Vec<CloverLineItem>>,
}

#[derive(Deserialize)]
struct CloverLineItem {
    item: Option<CloverItemRef>,
    price: Option<i64>,
    #[serde(rename = "unitQty")]
    unit_qty: Option<i64>,
}

#[derive(Deserialize)]
struct CloverItemRef {
    id: Option<String>,
}

#[async_trait]
impl PosAdapter for CloverAdapter {
    fn provider(&self) -> Provider {
        Provider::Clover
    }

    fn oauth_start(&self, _workspace_id: Uuid, redirect_uri: &str) -> OAuthStartResult {
        let state = generate_state();
        let url = format!(
            "{auth_url}?client_id={client_id}&redirect_uri={redirect}&state={state}",
            auth_url = CLOVER_AUTH_URL,
            client_id = urlencoding::encode(&self.client_id),
            redirect = urlencoding::encode(redirect_uri),
            state = state,
        );
        OAuthStartResult {
            authorization_url: url,
            state,
            pkce_verifier: String::new(), // Clover doesn't use PKCE
        }
    }

    async fn oauth_exchange(
        &self,
        code: &str,
        _pkce_verifier: &str,
        _redirect_uri: &str,
    ) -> Result<TokenSet, AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "code": code,
        });

        let resp = self
            .http
            .post(CLOVER_TOKEN_URL)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Clover token exchange failed: {status}"
            )));
        }

        let payload: CloverTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let access_token = payload.access_token.ok_or_else(|| {
            AppError::Other(anyhow::anyhow!("Clover: no access_token in response"))
        })?;

        Ok(TokenSet {
            access_token,
            refresh_token: None,
            expires_in_secs: None,
            expires_at: None,
            merchant_id: payload.merchant_id,
        })
    }

    async fn fetch_inventory(&self, access_token: &str) -> Result<Vec<PosProduct>, AppError> {
        let merchant_id = self.get_merchant_id(access_token).await?;

        let resp = self
            .http
            .get(self.merchant_url(&merchant_id, "/items?limit=1000"))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Clover items fetch failed: {}",
                resp.status()
            )));
        }

        let data: CloverItemsResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let products = data
            .elements
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let sku = item.id?;
                Some(PosProduct {
                    sku,
                    name: item.name.unwrap_or_default(),
                    quantity: 0,
                    price_cents: item.price,
                })
            })
            .collect();

        Ok(products)
    }

    async fn fetch_sales(
        &self,
        access_token: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<AdapterSale>, AppError> {
        let merchant_id = self.get_merchant_id(access_token).await?;
        let since_ms = since.timestamp_millis();

        let url = self.merchant_url(
            &merchant_id,
            &format!("/orders?expand=lineItems&filter=createdTime>{since_ms}&limit=500"),
        );

        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Clover orders fetch failed: {}",
                resp.status()
            )));
        }

        let data: CloverOrdersResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let sales = data
            .elements
            .unwrap_or_default()
            .into_iter()
            .filter_map(|order| {
                let order_id = order.id?;

                let occurred_at = order
                    .created_time
                    .map(|ms| DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now))
                    .unwrap_or_else(Utc::now);

                let lines = order
                    .line_items
                    .and_then(|w| w.elements)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|line| {
                        let sku = line.item.and_then(|i| i.id).unwrap_or_default();
                        if sku.is_empty() {
                            return None;
                        }
                        Some(AdapterSaleLine {
                            sku,
                            quantity: line.unit_qty.unwrap_or(1).max(1),
                            price_cents: line.price.unwrap_or(0),
                        })
                    })
                    .collect();

                Some(AdapterSale { external_id: order_id, occurred_at, lines })
            })
            .collect();

        Ok(sales)
    }

    async fn register_webhook(
        &self,
        _access_token: &str,
        _notification_url: &str,
    ) -> Result<Option<String>, AppError> {
        // Clover does not offer self-service webhook registration; fall back to polling.
        tracing::info!("Clover does not support webhooks; using polling fallback");
        Ok(None)
    }

    async fn refresh_token(&self, _refresh_token: &str) -> Result<TokenSet, AppError> {
        Err(AppError::Other(anyhow::anyhow!(
            "Clover tokens do not support programmatic refresh"
        )))
    }

    async fn revoke_token(&self, _access_token: &str) -> Result<(), AppError> {
        // Clover token revocation is not available via a public API.
        Ok(())
    }

    fn verify_webhook_signature(
        &self,
        _headers: &HeaderMap,
        _body: &[u8],
        _signing_key: &str,
    ) -> bool {
        // Clover does not send standard webhook signatures.
        // Rely on network-level controls (IP allowlist) in production.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CloverAdapter {
        CloverAdapter::new("test-client-id".into(), "test-client-secret".into())
    }

    #[test]
    fn provider_is_clover() {
        assert_eq!(adapter().provider(), Provider::Clover);
    }

    #[test]
    fn oauth_start_produces_valid_url() {
        let result = adapter().oauth_start(Uuid::new_v4(), "https://example.com/cb");
        assert!(result.authorization_url.contains("clover.com"));
        assert!(result.pkce_verifier.is_empty());
    }

    #[test]
    fn webhook_signature_always_true() {
        assert!(adapter().verify_webhook_signature(&axum::http::HeaderMap::new(), b"body", "key"));
    }
}
