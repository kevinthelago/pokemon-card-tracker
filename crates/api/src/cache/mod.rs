use anyhow::Result;
use redis::aio::ConnectionManager;

/// Connect to Redis if a URL is configured.
/// Redis is optional in development — velocity counters degrade gracefully without it.
pub async fn connect(url: Option<&str>) -> Result<Option<ConnectionManager>> {
    let Some(url) = url else {
        tracing::warn!("REDIS__URL not set; Redis features (velocity counters) disabled");
        return Ok(None);
    };

    let client = redis::Client::open(url)?;
    let conn = ConnectionManager::new(client).await?;
    tracing::info!("connected to Redis");
    Ok(Some(conn))
}
