//! PriceCharting API client — fallback pricing source and graded-price data.
//!
//! PriceCharting is used when TCGplayer embedded prices are unavailable and for
//! grade-specific graded-card pricing (PSA 1–10). Results are fetched on demand
//! and cached in the `valuations` table by the ValuationService.

use anyhow::{Context, Result};
use reqwest::Client;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, warn};

const BASE_URL: &str = "https://www.pricecharting.com/api";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct PriceChartingClient {
    client: Client,
    api_key: String,
}

// ── API response shapes ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ProductResponse {
    status: String,
    #[allow(dead_code)]
    id: Option<u64>,
    #[serde(rename = "product-name")]
    #[allow(dead_code)]
    product_name: Option<String>,
    /// Raw/loose price in cents (ungraded card).
    #[serde(rename = "loose-price")]
    loose_price: Option<u64>,
    /// Generic "graded" price in cents (aggregate, not grade-specific).
    #[serde(rename = "graded-price")]
    graded_price: Option<u64>,
    /// Remaining fields, including "grade-N-price" for N in 1–10.
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ProductSearchResponse {
    status: String,
    products: Option<Vec<ProductResult>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProductResult {
    pub id: u64,
    #[serde(rename = "product-name")]
    pub product_name: String,
    #[serde(rename = "console-name")]
    pub console_name: String,
}

// ── Price extraction helpers ───────────────────────────────────────────────

impl ProductResponse {
    /// Lookup a PSA grade-specific price (grade 1–10). Stored as "grade-N-price" in cents.
    pub fn grade_price(&self, grade: u8) -> Option<Decimal> {
        let key = format!("grade-{}-price", grade);
        self.extra
            .get(&key)
            .and_then(|v| v.as_u64())
            .map(cents_to_decimal)
    }

    pub fn loose_price_decimal(&self) -> Option<Decimal> {
        self.loose_price.map(cents_to_decimal)
    }

    pub fn graded_price_decimal(&self) -> Option<Decimal> {
        self.graded_price.map(cents_to_decimal)
    }

    /// Best available graded price: grade-specific first, then generic graded.
    pub fn best_graded_price(&self, grade: u8) -> Option<Decimal> {
        self.grade_price(grade)
            .or_else(|| self.graded_price_decimal())
    }
}

fn cents_to_decimal(cents: u64) -> Decimal {
    Decimal::from(cents) / Decimal::from(100u64)
}

// ── Client implementation ──────────────────────────────────────────────────

impl PriceChartingClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent("CardGuard/1.0 (+https://github.com/kevinthelago/pokemon-card-tracker)")
                .build()
                .expect("reqwest client build"),
            api_key: api_key.into(),
        }
    }

    /// Fetch a product by its PriceCharting numeric ID.
    pub async fn product_by_id(&self, pc_id: u64) -> Result<Option<ProductResponse>> {
        let url = format!("{}/product?id={}&api_key={}", BASE_URL, pc_id, self.api_key);
        let resp: ProductResponse = self
            .client
            .get(&url)
            .send()
            .await
            .context("PriceCharting product request")?
            .error_for_status()
            .context("PriceCharting product HTTP error")?
            .json()
            .await
            .context("PriceCharting product JSON decode")?;

        if resp.status != "200" {
            debug!(?pc_id, status = %resp.status, "PriceCharting product not found");
            return Ok(None);
        }
        Ok(Some(resp))
    }

    /// Search for products by name query. Returns all results (caller filters).
    pub async fn search(&self, query: &str) -> Result<Vec<ProductResult>> {
        let encoded = urlencoding::encode(query);
        let url = format!(
            "{}/products?q={}&api_key={}",
            BASE_URL, encoded, self.api_key
        );
        let resp: ProductSearchResponse = self
            .client
            .get(&url)
            .send()
            .await
            .context("PriceCharting search request")?
            .error_for_status()
            .context("PriceCharting search HTTP error")?
            .json()
            .await
            .context("PriceCharting search JSON decode")?;

        if resp.status != "200" {
            warn!(query, status = %resp.status, "PriceCharting search returned non-200");
            return Ok(vec![]);
        }
        Ok(resp.products.unwrap_or_default())
    }

    /// Find the first Pokémon product matching name + set.
    async fn find_pokemon_product(
        &self,
        card_name: &str,
        set_name: &str,
    ) -> Result<Option<ProductResult>> {
        let query = format!("{} {}", card_name, set_name);
        let results = self.search(&query).await?;
        Ok(results
            .into_iter()
            .find(|r| r.console_name.to_ascii_lowercase().contains("pokemon")))
    }

    /// Get the raw (ungraded) market price for a card. Returns None if not found.
    pub async fn raw_price(&self, card_name: &str, set_name: &str) -> Result<Option<Decimal>> {
        let Some(product) = self.find_pokemon_product(card_name, set_name).await? else {
            debug!(%card_name, %set_name, "No PriceCharting match for raw price");
            return Ok(None);
        };
        let Some(detail) = self.product_by_id(product.id).await? else {
            return Ok(None);
        };
        Ok(detail.loose_price_decimal())
    }

    /// Get a grade-specific price for a graded card.
    /// `grade` is rounded to the nearest integer (PSA grades 1–10).
    /// Falls back to the generic graded price if no grade-specific data exists.
    pub async fn graded_price(
        &self,
        card_name: &str,
        set_name: &str,
        grade: u8,
    ) -> Result<Option<Decimal>> {
        let Some(product) = self.find_pokemon_product(card_name, set_name).await? else {
            return Ok(None);
        };
        let Some(detail) = self.product_by_id(product.id).await? else {
            return Ok(None);
        };
        Ok(detail.best_graded_price(grade))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mock_product(
        loose_cents: Option<u64>,
        graded_cents: Option<u64>,
        grade_9_cents: Option<u64>,
    ) -> ProductResponse {
        let mut extra = HashMap::new();
        if let Some(c) = grade_9_cents {
            extra.insert("grade-9-price".into(), json!(c));
        }
        ProductResponse {
            status: "200".into(),
            id: Some(1),
            product_name: Some("Test Card".into()),
            loose_price: loose_cents,
            graded_price: graded_cents,
            extra,
        }
    }

    #[test]
    fn loose_price_converts_cents_to_decimal() {
        let p = mock_product(Some(1050), None, None);
        assert_eq!(
            p.loose_price_decimal(),
            Some(Decimal::from_str("10.50").unwrap())
        );
    }

    #[test]
    fn grade_price_extracts_specific_grade() {
        let p = mock_product(None, None, Some(25000));
        assert_eq!(p.grade_price(9), Some(Decimal::from_str("250.00").unwrap()));
    }

    #[test]
    fn best_graded_price_falls_back_to_generic() {
        let p = mock_product(None, Some(15000), None);
        // No grade-9-price, should return generic graded
        assert_eq!(
            p.best_graded_price(9),
            Some(Decimal::from_str("150.00").unwrap())
        );
    }

    #[test]
    fn best_graded_price_prefers_specific_grade() {
        let p = mock_product(None, Some(15000), Some(25000));
        assert_eq!(
            p.best_graded_price(9),
            Some(Decimal::from_str("250.00").unwrap())
        );
    }

    #[test]
    fn missing_price_returns_none() {
        let p = mock_product(None, None, None);
        assert_eq!(p.loose_price_decimal(), None);
        assert_eq!(p.grade_price(9), None);
    }
}
