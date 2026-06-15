use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::AppError,
    pos::adapter::{
        generate_state, verify_hmac_sha256_base64, AdapterSale, AdapterSaleLine, OAuthStartResult,
        PosAdapter, PosProduct, Provider, TokenSet,
    },
};

pub struct ShopifyAdapter {
    api_key: String,
    api_secret: String,
    shop_domain: String,
    http: reqwest::Client,
}

impl ShopifyAdapter {
    pub fn new(api_key: String, api_secret: String, shop_domain: String) -> Self {
        Self {
            api_key,
            api_secret,
            shop_domain,
            http: reqwest::Client::new(),
        }
    }

    fn api_url(&self, path: &str) -> String {
        format!("https://{}/admin/api/2024-01{}", self.shop_domain, path)
    }

    fn shop_url(&self, path: &str) -> String {
        format!("https://{}{}", self.shop_domain, path)
    }
}

#[derive(Deserialize)]
struct ShopifyTokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct ShopifyProductsResponse {
    products: Option<Vec<ShopifyProduct>>,
}

#[derive(Deserialize)]
struct ShopifyProduct {
    title: Option<String>,
    variants: Option<Vec<ShopifyVariant>>,
}

#[derive(Deserialize)]
struct ShopifyVariant {
    sku: Option<String>,
    inventory_quantity: Option<i64>,
    price: Option<String>,
}

#[derive(Deserialize)]
struct ShopifyOrdersResponse {
    orders: Option<Vec<ShopifyOrder>>,
}

#[derive(Deserialize)]
struct ShopifyOrder {
    id: i64,
    created_at: Option<String>,
    line_items: Option<Vec<ShopifyLineItem>>,
}

#[derive(Deserialize)]
struct ShopifyLineItem {
    sku: Option<String>,
    quantity: Option<i64>,
    price: Option<String>,
}

#[derive(Deserialize)]
struct ShopifyWebhookResponse {
    webhook: Option<ShopifyWebhook>,
}

#[derive(Deserialize)]
struct ShopifyWebhook {
    id: Option<i64>,
}

#[async_trait]
impl PosAdapter for ShopifyAdapter {
    fn provider(&self) -> Provider {
        Provider::Shopify
    }

    fn oauth_start(&self, _workspace_id: Uuid, redirect_uri: &str) -> OAuthStartResult {
        let state = generate_state();
        let scopes = "read_products,read_inventory,read_orders";
        let url = format!(
            "https://{shop}/admin/oauth/authorize?client_id={key}&scope={scopes}\
             &redirect_uri={redirect}&state={state}",
            shop = self.shop_domain,
            key = urlencoding::encode(&self.api_key),
            scopes = scopes,
            redirect = urlencoding::encode(redirect_uri),
            state = state,
        );
        OAuthStartResult {
            authorization_url: url,
            state,
            pkce_verifier: String::new(), // Shopify doesn't use PKCE
        }
    }

    async fn oauth_exchange(
        &self,
        code: &str,
        _pkce_verifier: &str,
        _redirect_uri: &str,
    ) -> Result<TokenSet, AppError> {
        let body = serde_json::json!({
            "client_id": self.api_key,
            "client_secret": self.api_secret,
            "code": code,
        });

        let resp = self
            .http
            .post(self.shop_url("/admin/oauth/access_token"))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Shopify token exchange failed: {status}"
            )));
        }

        let payload: ShopifyTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let access_token = payload.access_token.ok_or_else(|| {
            AppError::Other(anyhow::anyhow!("Shopify: no access_token in response"))
        })?;

        Ok(TokenSet {
            access_token,
            refresh_token: None,
            expires_in_secs: None,
            expires_at: None,
            merchant_id: Some(self.shop_domain.clone()),
        })
    }

    async fn fetch_inventory(&self, access_token: &str) -> Result<Vec<PosProduct>, AppError> {
        let resp = self
            .http
            .get(self.api_url("/products.json?limit=250"))
            .header("X-Shopify-Access-Token", access_token)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Shopify products fetch failed: {}",
                resp.status()
            )));
        }

        let data: ShopifyProductsResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let mut products = Vec::new();
        for product in data.products.unwrap_or_default() {
            let name = product.title.unwrap_or_default();
            for variant in product.variants.unwrap_or_default() {
                let sku = variant.sku.unwrap_or_default();
                if sku.is_empty() {
                    continue;
                }
                let price_cents = variant
                    .price
                    .as_deref()
                    .and_then(|p| p.parse::<f64>().ok())
                    .map(|p| (p * 100.0) as i64);
                products.push(PosProduct {
                    sku,
                    name: name.clone(),
                    quantity: variant.inventory_quantity.unwrap_or(0),
                    price_cents,
                });
            }
        }
        Ok(products)
    }

    async fn fetch_sales(
        &self,
        access_token: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<AdapterSale>, AppError> {
        let url = self.api_url(&format!(
            "/orders.json?status=any&limit=250&updated_at_min={}",
            urlencoding::encode(&since.to_rfc3339())
        ));

        let resp = self
            .http
            .get(url)
            .header("X-Shopify-Access-Token", access_token)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Shopify orders fetch failed: {}",
                resp.status()
            )));
        }

        let data: ShopifyOrdersResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let sales = data
            .orders
            .unwrap_or_default()
            .into_iter()
            .map(|order| {
                let occurred_at = order
                    .created_at
                    .as_deref()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);

                let lines = order
                    .line_items
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|item| {
                        let sku = item.sku.unwrap_or_default();
                        if sku.is_empty() {
                            return None;
                        }
                        let quantity = item.quantity.unwrap_or(1);
                        let price_cents = item
                            .price
                            .as_deref()
                            .and_then(|p| p.parse::<f64>().ok())
                            .map(|p| (p * 100.0) as i64)
                            .unwrap_or(0);
                        Some(AdapterSaleLine {
                            sku,
                            quantity,
                            price_cents,
                        })
                    })
                    .collect();

                AdapterSale {
                    external_id: order.id.to_string(),
                    occurred_at,
                    lines,
                }
            })
            .collect();

        Ok(sales)
    }

    async fn register_webhook(
        &self,
        access_token: &str,
        notification_url: &str,
    ) -> Result<Option<String>, AppError> {
        let body = serde_json::json!({
            "webhook": {
                "topic": "orders/create",
                "address": notification_url,
                "format": "json"
            }
        });

        let resp = self
            .http
            .post(self.api_url("/webhooks.json"))
            .header("X-Shopify-Access-Token", access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if !resp.status().is_success() {
            tracing::warn!("Shopify webhook registration failed: {}", resp.status());
            return Ok(None);
        }

        let data: ShopifyWebhookResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        Ok(data.webhook.and_then(|w| w.id).map(|id| id.to_string()))
    }

    async fn refresh_token(&self, _refresh_token: &str) -> Result<TokenSet, AppError> {
        // Shopify access tokens are long-lived and don't support programmatic refresh.
        Err(AppError::Other(anyhow::anyhow!(
            "Shopify tokens do not support refresh"
        )))
    }

    async fn revoke_token(&self, _access_token: &str) -> Result<(), AppError> {
        // Shopify token revocation happens via the Partners dashboard, not an API call.
        Ok(())
    }

    fn verify_webhook_signature(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        signing_key: &str,
    ) -> bool {
        let sig = match headers
            .get("x-shopify-hmac-sha256")
            .and_then(|v| v.to_str().ok())
        {
            Some(s) => s,
            None => return false,
        };
        verify_hmac_sha256_base64(signing_key.as_bytes(), body, sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> ShopifyAdapter {
        ShopifyAdapter::new(
            "test-api-key".into(),
            "test-api-secret".into(),
            "test-shop.myshopify.com".into(),
        )
    }

    #[test]
    fn provider_is_shopify() {
        assert_eq!(adapter().provider(), Provider::Shopify);
    }

    #[test]
    fn oauth_start_produces_valid_url() {
        let result = adapter().oauth_start(Uuid::new_v4(), "https://example.com/cb");
        assert!(result.authorization_url.contains("test-shop.myshopify.com"));
        assert!(result.authorization_url.contains("test-api-key"));
        assert!(result.pkce_verifier.is_empty());
    }

    #[test]
    fn webhook_invalid_when_no_header() {
        assert!(!adapter().verify_webhook_signature(&axum::http::HeaderMap::new(), b"body", "key"));
    }
}
