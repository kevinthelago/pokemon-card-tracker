use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::{error::AppError, integrations::pokemontcg::PokemonTcgClient};
use domain::{Printing, SealedProduct};

// ── Request types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    /// Free-form Lucene query forwarded to the TCG API.
    #[serde(default)]
    pub q: Option<String>,
    /// Card name (partial match, case-insensitive).
    #[serde(default)]
    pub name: Option<String>,
    /// Set ID filter (e.g. `"swsh1"`).
    #[serde(default)]
    pub set: Option<String>,
    /// Card number within the set (e.g. `"57"`).
    #[serde(default)]
    pub number: Option<String>,
    /// Language code (default `"en"`).
    #[serde(default)]
    pub language: Option<String>,
    /// Edition filter (e.g. `"1st Edition"`).
    #[serde(default)]
    pub edition: Option<String>,
    /// Opaque cursor returned by a previous response.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size (default 20, max 100).
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResults {
    pub items: Vec<Printing>,
    pub total_count: u32,
    /// Opaque next-page cursor; absent when no more pages.
    pub next_cursor: Option<String>,
}

// ── DB row type (runtime sqlx mapping) ────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct PrintingRow {
    pub id: Uuid,
    pub tcg_api_id: String,
    pub name: String,
    pub set_id: String,
    pub set_name: String,
    pub number: String,
    pub variant: Option<String>,
    pub language: String,
    pub edition: Option<String>,
    pub image_url: Option<String>,
    pub image_url_large: Option<String>,
    pub supertype: Option<String>,
    pub rarity: Option<String>,
    pub cached_at: DateTime<Utc>,
}

impl From<PrintingRow> for Printing {
    fn from(r: PrintingRow) -> Self {
        Printing {
            id: r.id,
            tcg_api_id: r.tcg_api_id,
            name: r.name,
            set_id: r.set_id,
            set_name: r.set_name,
            number: r.number,
            variant: r.variant,
            language: r.language,
            edition: r.edition,
            image_url: r.image_url,
            image_url_large: r.image_url_large,
            supertype: r.supertype,
            rarity: r.rarity,
            cached_at: r.cached_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct SealedProductRow {
    pub id: Uuid,
    pub upc: String,
    pub name: String,
    pub set_id: Option<String>,
    pub product_type: String,
    pub cached_at: DateTime<Utc>,
}

impl From<SealedProductRow> for SealedProduct {
    fn from(r: SealedProductRow) -> Self {
        SealedProduct {
            id: r.id,
            upc: r.upc,
            name: r.name,
            set_id: r.set_id,
            product_type: r.product_type,
            cached_at: r.cached_at,
        }
    }
}

// ── Service ────────────────────────────────────────────────────────────────

pub struct IdentityService {
    db: PgPool,
    tcg: Arc<PokemonTcgClient>,
}

impl IdentityService {
    pub fn new(db: PgPool, tcg: Arc<PokemonTcgClient>) -> Self {
        Self { db, tcg }
    }

    /// Search for printings — DB cache first, TCG API fallback.
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResults, AppError> {
        let tcg_q = build_tcg_query(query);

        if tcg_q.is_empty() {
            return Ok(SearchResults {
                items: vec![],
                total_count: 0,
                next_cursor: None,
            });
        }

        // Cursor is an encoded page number.
        let page = query
            .cursor
            .as_ref()
            .and_then(|c| c.parse::<u32>().ok())
            .unwrap_or(1);

        // Check DB cache (valid for 7 days).
        let cached = self.search_cache(&tcg_q, page as i64, query.limit).await?;
        if !cached.is_empty() {
            let next = if cached.len() as i64 >= query.limit {
                Some((page + 1).to_string())
            } else {
                None
            };
            let count = cached.len() as u32;
            return Ok(SearchResults {
                items: cached,
                total_count: count,
                next_cursor: next,
            });
        }

        // Cache miss — call TCG API.
        let resp = self
            .tcg
            .search(&tcg_q, page, query.limit as u32)
            .await
            .map_err(|e| AppError::ExternalApi(e.to_string()))?;

        let printings = self.cache_printings(&resp.data).await?;

        let next = if resp.count >= query.limit as u32 {
            Some((page + 1).to_string())
        } else {
            None
        };

        Ok(SearchResults {
            items: printings,
            total_count: resp.total_count,
            next_cursor: next,
        })
    }

    /// Look up a printing by its DB id.
    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<Printing>, AppError> {
        let row = sqlx::query_as::<_, PrintingRow>(
            "SELECT id, tcg_api_id, name, set_id, set_name, number, variant, language,
                    edition, image_url, image_url_large, supertype, rarity, cached_at
             FROM printings
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::Sqlx)?;

        Ok(row.map(Printing::from))
    }

    /// Resolve a UPC to a sealed product (DB-only in v1; returns None if unknown).
    pub async fn resolve_upc(&self, upc: &str) -> Result<Option<SealedProduct>, AppError> {
        let row = sqlx::query_as::<_, SealedProductRow>(
            "SELECT id, upc, name, set_id, product_type, cached_at
             FROM sealed_products
             WHERE upc = $1",
        )
        .bind(upc)
        .fetch_optional(&self.db)
        .await
        .map_err(AppError::Sqlx)?;

        Ok(row.map(SealedProduct::from))
    }

    // ── Private helpers ────────────────────────────────────────────────────

    async fn search_cache(
        &self,
        q: &str,
        page: i64,
        limit: i64,
    ) -> Result<Vec<Printing>, AppError> {
        let offset = (page - 1).max(0) * limit;

        // Match cached printings whose name contains the query terms.
        // This is a simple heuristic: accurate for name-based queries; the TCG
        // API is the authoritative source for complex Lucene queries.
        let rows = sqlx::query_as::<_, PrintingRow>(
            "SELECT id, tcg_api_id, name, set_id, set_name, number, variant, language,
                    edition, image_url, image_url_large, supertype, rarity, cached_at
             FROM printings
             WHERE cached_at > NOW() - INTERVAL '7 days'
               AND (name ILIKE $1 OR tcg_api_id = $2)
             ORDER BY name, set_id, number
             LIMIT $3 OFFSET $4",
        )
        .bind(format!("%{}%", q))
        .bind(q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(AppError::Sqlx)?;

        Ok(rows.into_iter().map(Printing::from).collect())
    }

    async fn cache_printings(
        &self,
        cards: &[crate::integrations::pokemontcg::TcgCard],
    ) -> Result<Vec<Printing>, AppError> {
        let mut result = Vec::with_capacity(cards.len());

        for card in cards {
            let raw = serde_json::to_value(card)
                .map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;

            let row = sqlx::query_as::<_, PrintingRow>(
                "INSERT INTO printings (
                    tcg_api_id, name, set_id, set_name, number,
                    image_url, image_url_large, supertype, rarity,
                    language, raw_data
                 )
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'en', $10)
                 ON CONFLICT (tcg_api_id) DO UPDATE SET
                    name           = EXCLUDED.name,
                    set_name       = EXCLUDED.set_name,
                    image_url      = EXCLUDED.image_url,
                    image_url_large = EXCLUDED.image_url_large,
                    supertype      = EXCLUDED.supertype,
                    rarity         = EXCLUDED.rarity,
                    raw_data       = EXCLUDED.raw_data,
                    cached_at      = NOW()
                 RETURNING id, tcg_api_id, name, set_id, set_name, number, variant, language,
                           edition, image_url, image_url_large, supertype, rarity, cached_at",
            )
            .bind(&card.id)
            .bind(&card.name)
            .bind(&card.set.id)
            .bind(&card.set.name)
            .bind(&card.number)
            .bind(Some(&card.images.small))
            .bind(Some(&card.images.large))
            .bind(card.supertype.as_deref())
            .bind(card.rarity.as_deref())
            .bind(raw)
            .fetch_one(&self.db)
            .await
            .map_err(AppError::Sqlx)?;

            result.push(Printing::from(row));
        }

        Ok(result)
    }
}

// ── Query builder ──────────────────────────────────────────────────────────

/// Translate a `SearchQuery` into a TCG API Lucene query string.
pub fn build_tcg_query(q: &SearchQuery) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(raw) = &q.q {
        parts.push(raw.clone());
    }
    if let Some(name) = &q.name {
        let safe = name.replace('"', "");
        parts.push(format!("name:\"{}*\"", safe));
    }
    if let Some(set) = &q.set {
        // Set IDs are alphanumeric — strip quotes and spaces.
        let safe = set.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
        parts.push(format!("set.id:{}", safe));
    }
    if let Some(number) = &q.number {
        let safe = number.replace('"', "").replace(' ', "");
        parts.push(format!("number:{}", safe));
    }

    parts.join(" AND ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_query_name_only() {
        let q = SearchQuery {
            q: None,
            name: Some("Pikachu".into()),
            set: None,
            number: None,
            language: None,
            edition: None,
            cursor: None,
            limit: 20,
        };
        assert_eq!(build_tcg_query(&q), r#"name:"Pikachu*""#);
    }

    #[test]
    fn build_query_combined() {
        let q = SearchQuery {
            q: None,
            name: Some("Charizard".into()),
            set: Some("swsh1".into()),
            number: Some("20".into()),
            language: None,
            edition: None,
            cursor: None,
            limit: 20,
        };
        let result = build_tcg_query(&q);
        assert!(result.contains("name:\"Charizard*\""));
        assert!(result.contains("set.id:swsh1"));
        assert!(result.contains("number:20"));
    }

    #[test]
    fn build_query_sanitizes_quotes() {
        let q = SearchQuery {
            q: None,
            name: Some(r#"Bad"Name"#.into()),
            set: None,
            number: None,
            language: None,
            edition: None,
            cursor: None,
            limit: 20,
        };
        let result = build_tcg_query(&q);
        assert!(!result.contains(r#"\""#));
    }

    #[test]
    fn empty_query_returns_empty_string() {
        let q = SearchQuery {
            q: None,
            name: None,
            set: None,
            number: None,
            language: None,
            edition: None,
            cursor: None,
            limit: 20,
        };
        assert_eq!(build_tcg_query(&q), "");
    }
}
