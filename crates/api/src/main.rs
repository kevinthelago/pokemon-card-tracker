mod app;
mod config;
mod db;
mod error;
mod grading;

use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = config::Config::from_env()?;

    let pool = db::connect(&cfg.database_url).await?;
    tracing::info!("database connected");

    let grading_svc = Arc::new(grading::GradingService::new(
        pool.clone(),
        cfg.psa_api_key.clone(),
        cfg.grading_cache_ttl_secs,
    ));

    let state = app::AppState {
        db: pool,
        grading: grading_svc,
    };

    let addr = cfg.listen_addr.clone();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("API listening on {}", addr);

    axum::serve(listener, app::build_router(state)).await?;

    Ok(())
}
