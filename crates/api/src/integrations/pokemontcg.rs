use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use thiserror::Error;

const BASE_URL: &str = "https://api.pokemontcg.io/v2";
const MAX_PAGE_SIZE: u32 = 250;

#[derive(Debug, Error)]
pub enum TcgApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API returned {status}: {message}")]
    ApiError { status: u16, message: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// ── Wire types matching the Pokémon TCG API v2 response ───────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcgCard {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub supertype: Option<String>,
    #[serde(default)]
    pub subtypes: Option<Vec<String>>,
    #[serde(default)]
    pub hp: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    pub number: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub rarity: Option<String>,
    pub set: TcgSet,
    pub images: TcgCardImages,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcgSet {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default, rename = "printedTotal")]
    pub printed_total: Option<u32>,
    pub total: u32,
    #[serde(default, rename = "releaseDate")]
    pub release_date: Option<String>,
    pub images: TcgSetImages,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcgCardImages {
    pub small: String,
    pub large: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcgSetImages {
    pub symbol: String,
    pub logo: String,
}

#[derive(Debug, Deserialize)]
pub struct TcgSearchResponse {
    pub data: Vec<TcgCard>,
    pub page: u32,
    #[serde(rename = "pageSize")]
    pub page_size: u32,
    pub count: u32,
    #[serde(rename = "totalCount")]
    pub total_count: u32,
}

#[derive(Debug, Deserialize)]
struct TcgSingleResponse {
    pub data: TcgCard,
}

// ── Client ─────────────────────────────────────────────────────────────────

pub struct PokemonTcgClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    // 100 req/min default; bumped to 1 000/min with an API key.
    limiter: Arc<DefaultDirectRateLimiter>,
}

impl PokemonTcgClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(BASE_URL, api_key)
    }

    pub fn with_base_url(base_url: &str, api_key: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");

        let rpm = if api_key.is_some() { 1_000u32 } else { 100u32 };
        let quota = Quota::per_minute(NonZeroU32::new(rpm).unwrap());
        let limiter = Arc::new(RateLimiter::direct(quota));

        Self {
            client,
            base_url: base_url.to_string(),
            api_key,
            limiter,
        }
    }

    /// Search cards using the Pokémon TCG API Lucene query syntax.
    pub async fn search(
        &self,
        q: &str,
        page: u32,
        page_size: u32,
    ) -> Result<TcgSearchResponse, TcgApiError> {
        self.limiter.until_ready().await;

        let page_size = page_size.min(MAX_PAGE_SIZE);
        let mut req = self
            .client
            .get(format!("{}/cards", self.base_url))
            .query(&[
                ("q", q.to_string()),
                ("page", page.to_string()),
                ("pageSize", page_size.to_string()),
            ]);

        if let Some(key) = &self.api_key {
            req = req.header("X-Api-Key", key);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(TcgApiError::ApiError { status, message });
        }

        Ok(resp.json().await?)
    }

    /// Fetch a single card by its composite ID (e.g. `"swsh1-57"`).
    pub async fn get_card(&self, id: &str) -> Result<Option<TcgCard>, TcgApiError> {
        self.limiter.until_ready().await;

        let mut req = self
            .client
            .get(format!("{}/cards/{}", self.base_url, id));

        if let Some(key) = &self.api_key {
            req = req.header("X-Api-Key", key);
        }

        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(TcgApiError::ApiError { status, message });
        }

        let body: TcgSingleResponse = resp.json().await?;
        Ok(Some(body.data))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()))
    }

    #[tokio::test]
    async fn search_returns_cards() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/cards").query_param("q", "name:\"Pikachu*\"");
            then.status(200)
                .header("content-type", "application/json")
                .body(fixture("pokemontcg_search.json"));
        });

        let client = PokemonTcgClient::with_base_url(&server.base_url(), None);
        let resp = client.search("name:\"Pikachu*\"", 1, 20).await.unwrap();

        assert_eq!(resp.count, 1);
        assert_eq!(resp.data[0].name, "Pikachu");
        assert_eq!(resp.data[0].set.id, "swsh1");
    }

    #[tokio::test]
    async fn get_card_returns_none_on_404() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/cards/bad-id");
            then.status(404).body("{}");
        });

        let client = PokemonTcgClient::with_base_url(&server.base_url(), None);
        let result = client.get_card("bad-id").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_card_returns_card() {
        let server = MockServer::start();

        server.mock(|when, then| {
            when.method(GET).path("/cards/swsh1-57");
            then.status(200)
                .header("content-type", "application/json")
                .body(fixture("pokemontcg_card.json"));
        });

        let client = PokemonTcgClient::with_base_url(&server.base_url(), None);
        let card = client.get_card("swsh1-57").await.unwrap().unwrap();
        assert_eq!(card.id, "swsh1-57");
    }
}
