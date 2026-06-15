use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    pub encryption: EncryptionConfig,
    pub external: ExternalApiConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".into()
}
fn default_port() -> u16 {
    3000
}

impl ServerConfig {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_pool_max")]
    pub max_connections: u32,
}

fn default_pool_max() -> u32 {
    20
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    /// HS256 signing secret for JWT tokens. Must be at least 32 bytes.
    pub jwt_secret: String,
    /// Access token lifetime in seconds (default 15 min).
    #[serde(default = "default_access_ttl")]
    pub access_token_ttl_secs: u64,
    /// Refresh token lifetime in seconds (default 30 days).
    #[serde(default = "default_refresh_ttl")]
    pub refresh_token_ttl_secs: u64,
}

fn default_access_ttl() -> u64 {
    900
}
fn default_refresh_ttl() -> u64 {
    30 * 24 * 3600
}

#[derive(Debug, Deserialize, Clone)]
pub struct EncryptionConfig {
    /// 32-byte AES-256-GCM key encoded as lowercase hex. Used for column-level
    /// encryption of POS OAuth tokens and buyer PII.
    pub key_hex: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExternalApiConfig {
    pub pokemon_tcg_api_key: Option<String>,
    pub pricecharting_api_key: Option<String>,
    pub psa_api_key: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let cfg = config::Config::builder()
            .add_source(
                config::Environment::default()
                    .separator("__")
                    .try_parsing(true),
            )
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3000)?
            .set_default("database.max_connections", 20)?
            .set_default("auth.access_token_ttl_secs", 900)?
            .set_default("auth.refresh_token_ttl_secs", 2_592_000)?
            .build()
            .context("failed to build configuration")?;

        cfg.try_deserialize()
            .context("failed to deserialise configuration")
    }
}
