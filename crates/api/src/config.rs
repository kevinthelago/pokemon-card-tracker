use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub jwt_secret: String,
    #[serde(default = "default_max_upload_bytes")]
    pub max_upload_bytes: usize,
    pub export_dir: String,
}

fn default_max_upload_bytes() -> usize {
    50 * 1024 * 1024 // 50 MB
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(config::Config::builder()
            .add_source(config::Environment::default())
            .set_default("port", 3000_i64)?
            .set_default("export_dir", "/tmp/cardguard-exports")?
            .set_default("jwt_secret", "dev-secret-change-me")?
            .set_default(
                "database_url",
                "postgres://cardguard:cardguard@localhost/cardguard",
            )?
            .build()?
            .try_deserialize()?)
    }
}
