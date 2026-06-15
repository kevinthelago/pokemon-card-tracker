use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::AppError,
    pos::adapter::{
        generate_pkce_pair, generate_state, verify_hmac_sha256_base64, AdapterSale,
        AdapterSaleLine, OAuthStartResult, PosAdapter, PosProduct, Provider, TokenSet,
    },
};

const SQUARE_AUTH_URL: &str = "https://connect.squareup.com/oauth2/authorize";
const SQUARE_TOKEN_URL: &str = "https://connect.squareup.com/oauth2/token";
const SQUARE_API_BASE: &str = "https://connect.squareup.com";
const SQUARE_VERSION: &str = "2023-10-18";
const SQUARE_SCOPE: &str = "MERCHANT_PROFILE_READ ITEMS_READ ORDERS_READ INVENTORY_READ";

pub struct SquareAdapter {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
}

impl SquareAdapter {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Deserialize)]
struct SquareTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<String>,
    merchant_id: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct SquareCatalogResponse {
    objects: Option<Vec<SquareCatalogObject>>,
}

#[derive(Deserialize)]
struct SquareCatalogObject {
    #[serde(rename = "type")]
    kind: String,
    item_data: Option<SquareItemData>,
}

#[derive(Deserialize)]
struct SquareItemData {
    name: Option<String>,
    variations: Option<Vec<SquareVariation>>,
}

#[derive(Deserialize)]
struct SquareVariation {
    item_variation_data: Option<SquareVariationData>,
}

#[derive(Deserialize)]
struct SquareVariationData {
    sku: Option<String>,
    price_money: Option<SquareMoney>,
}

#[derive(Deserialize)]
struct SquareMoney {
    amount: Option<i64>,
}

#[derive(Deserialize)]
struct SquareOrderSearchResponse {
    orders: Option<Vec<SquareOrder>>,
}

#[derive(Deserialize)]
struct SquareOrder {
    id: String,
    created_at: Option<String>,
    line_items: Option<Vec<SquareLineItem>>,
}

#[derive(Deserialize)]
struct SquareLineItem {
    catalog_object_id: Option<String>,
    name: Option<String>,
    quantity: Option<String>,
    base_price_money: Option<SquareMoney>,
}

#[derive(Serialize)]
struct SquareWebhookSubscriptionRequest<'a> {
    idempotency_key: String,
    subscription: SquareWebhookSubscription<'a>,
}

#[derive(Serialize)]
struct SquareWebhookSubscription<'a> {
    name: &'static str,
    enabled: bool,
    notification_url: &'a str,
    event_types: Vec<&'static str>,
}

#[derive(Deserialize)]
struct SquareWebhookSubscriptionResponse {
    subscription: Option<SquareWebhookSubscriptionResult>,
}

#[derive(Deserialize)]
struct SquareWebhookSubscriptionResult {
    id: String,
}

#[async_trait]
impl PosAdapter for SquareAdapter {
    fn provider(&self) -> Provider {
        Provider::Square
    }

    fn oauth_start(&self, _workspace_id: Uuid, redirect_uri: &str) -> OAuthStartResult {
        let state = generate_state();
        let (verifier, challenge) = generate_pkce_pair();
        let url = format!(
            "{auth_url}?client_id={client_id}&scope={scope}&redirect_uri={redirect}&state={state}\
             &code_challenge={challenge}&code_challenge_method=S256&session=false",
            auth_url = SQUARE_AUTH_URL,
            client_id = urlencoding::encode(&self.client_id),
            scope = urlencoding::encode(SQUARE_SCOPE),
            redirect = urlencoding::encode(redirect_uri),
            state = state,
            challenge = challenge,
        );
        OAuthStartResult {
            authorization_url: url,
            state,
            pkce_verifier: verifier,
        }
    }

    async fn oauth_exchange(
        &self,
        code: &str,
        pkce_verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenSet, AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": redirect_uri,
            "code_verifier": pkce_verifier,
        });

        let resp = self
            .http
            .post(SQUARE_TOKEN_URL)
            .header("Square-Version", SQUARE_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let status = resp.status();
        let payload: SquareTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if !status.is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Square token exchange failed: {}",
                payload
                    .error_description
                    .or(payload.error)
                    .unwrap_or_else(|| status.to_string())
            )));
        }

        let access_token = payload.access_token.ok_or_else(|| {
            AppError::Other(anyhow::anyhow!("Square: no access_token in response"))
        })?;

        let expires_at = payload
            .expires_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(TokenSet {
            access_token,
            refresh_token: payload.refresh_token,
            expires_in_secs: None,
            expires_at,
            merchant_id: payload.merchant_id,
        })
    }

    async fn fetch_inventory(&self, access_token: &str) -> Result<Vec<PosProduct>, AppError> {
        let resp = self
            .http
            .get(format!("{SQUARE_API_BASE}/v2/catalog/list?types=ITEM"))
            .bearer_auth(access_token)
            .header("Square-Version", SQUARE_VERSION)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Square catalog list failed: {}",
                resp.status()
            )));
        }

        let catalog: SquareCatalogResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let mut products = Vec::new();
        for obj in catalog.objects.unwrap_or_default() {
            if obj.kind != "ITEM" {
                continue;
            }
            let Some(item) = obj.item_data else { continue };
            let name = item.name.unwrap_or_default();
            for variation in item.variations.unwrap_or_default() {
                let Some(vdata) = variation.item_variation_data else {
                    continue;
                };
                let sku = vdata.sku.unwrap_or_default();
                if sku.is_empty() {
                    continue;
                }
                products.push(PosProduct {
                    sku,
                    name: name.clone(),
                    quantity: 0,
                    price_cents: vdata.price_money.and_then(|m| m.amount),
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
        // Fetch location IDs first
        let loc_resp = self
            .http
            .get(format!("{SQUARE_API_BASE}/v2/locations"))
            .bearer_auth(access_token)
            .header("Square-Version", SQUARE_VERSION)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let loc_json: serde_json::Value = loc_resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let location_ids: Vec<String> = loc_json["locations"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|l| l["id"].as_str().map(|s| s.to_owned()))
            .collect();

        if location_ids.is_empty() {
            return Ok(vec![]);
        }

        let body = serde_json::json!({
            "location_ids": location_ids,
            "query": {
                "filter": {
                    "date_time_filter": {
                        "created_at": { "start_at": since.to_rfc3339() }
                    }
                }
            },
            "return_entries": false
        });

        let resp = self
            .http
            .post(format!("{SQUARE_API_BASE}/v2/orders/search"))
            .bearer_auth(access_token)
            .header("Square-Version", SQUARE_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }
        if !resp.status().is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Square orders search failed: {}",
                resp.status()
            )));
        }

        let order_resp: SquareOrderSearchResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        let sales = order_resp
            .orders
            .unwrap_or_default()
            .into_iter()
            .filter_map(|order| {
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
                        let sku = item.catalog_object_id.or(item.name).unwrap_or_default();
                        if sku.is_empty() {
                            return None;
                        }
                        let quantity: i64 = item
                            .quantity
                            .as_deref()
                            .and_then(|q| q.parse::<f64>().ok())
                            .unwrap_or(1.0) as i64;
                        let price_cents = item.base_price_money.and_then(|m| m.amount).unwrap_or(0);
                        Some(AdapterSaleLine {
                            sku,
                            quantity,
                            price_cents,
                        })
                    })
                    .collect();

                Some(AdapterSale {
                    external_id: order.id,
                    occurred_at,
                    lines,
                })
            })
            .collect();

        Ok(sales)
    }

    async fn register_webhook(
        &self,
        access_token: &str,
        notification_url: &str,
    ) -> Result<Option<String>, AppError> {
        let body = SquareWebhookSubscriptionRequest {
            idempotency_key: Uuid::new_v4().to_string(),
            subscription: SquareWebhookSubscription {
                name: "CardGuard",
                enabled: true,
                notification_url,
                event_types: vec!["order.created", "inventory.count.updated"],
            },
        };

        let resp = self
            .http
            .post(format!("{SQUARE_API_BASE}/v2/webhooks/subscriptions"))
            .bearer_auth(access_token)
            .header("Square-Version", SQUARE_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if !resp.status().is_success() {
            tracing::warn!("Square webhook registration failed: {}", resp.status());
            return Ok(None);
        }

        let payload: SquareWebhookSubscriptionResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        Ok(payload.subscription.map(|s| s.id))
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenSet, AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        });

        let resp = self
            .http
            .post(SQUARE_TOKEN_URL)
            .header("Square-Version", SQUARE_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if resp.status() == 401 {
            return Err(AppError::Unauthorized);
        }

        let status = resp.status();
        let payload: SquareTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!(e.to_string())))?;

        if !status.is_success() {
            return Err(AppError::Other(anyhow::anyhow!(
                "Square token refresh failed: {}",
                payload
                    .error_description
                    .or(payload.error)
                    .unwrap_or_else(|| status.to_string())
            )));
        }

        let access_token = payload.access_token.ok_or_else(|| {
            AppError::Other(anyhow::anyhow!(
                "Square: no access_token in refresh response"
            ))
        })?;

        Ok(TokenSet {
            access_token,
            refresh_token: payload.refresh_token,
            expires_in_secs: None,
            expires_at: payload
                .expires_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            merchant_id: payload.merchant_id,
        })
    }

    async fn revoke_token(&self, access_token: &str) -> Result<(), AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "access_token": access_token,
        });
        let _ = self
            .http
            .post(format!("{SQUARE_API_BASE}/oauth2/revoke"))
            .header("Authorization", format!("Client {}", self.client_secret))
            .header("Square-Version", SQUARE_VERSION)
            .json(&body)
            .send()
            .await;
        Ok(())
    }

    fn verify_webhook_signature(
        &self,
        headers: &HeaderMap,
        body: &[u8],
        signing_key: &str,
    ) -> bool {
        let sig = match headers
            .get("x-square-hmacsha256-signature")
            .and_then(|v| v.to_str().ok())
        {
            Some(s) => s,
            None => return false,
        };
        // Square signs: notification_url + raw_body
        verify_hmac_sha256_base64(signing_key.as_bytes(), body, sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> SquareAdapter {
        SquareAdapter::new("test-client-id".into(), "test-client-secret".into())
    }

    #[test]
    fn provider_is_square() {
        assert_eq!(adapter().provider(), Provider::Square);
    }

    #[test]
    fn oauth_start_produces_valid_url() {
        let result = adapter().oauth_start(Uuid::new_v4(), "https://example.com/callback");
        assert!(result.authorization_url.contains("connect.squareup.com"));
        assert!(result.authorization_url.contains("test-client-id"));
        assert!(result
            .authorization_url
            .contains("code_challenge_method=S256"));
        assert!(!result.state.is_empty());
        assert!(!result.pkce_verifier.is_empty());
    }

    #[test]
    fn webhook_signature_invalid_when_key_wrong() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-square-hmacsha256-signature", "badsig==".parse().unwrap());
        assert!(!adapter().verify_webhook_signature(&headers, b"test body", "wrong-key"));
    }

    #[test]
    fn webhook_signature_invalid_when_no_header() {
        assert!(!adapter().verify_webhook_signature(&axum::http::HeaderMap::new(), b"body", "key"));
    }
}
