use std::sync::Arc;

use anyhow::Context;
use axum::Extension;
use cardguard_api::{
    app, auth, catalogue, crypto::EncryptionKey, grading::GradingService,
    integrations::pokemontcg::PokemonTcgClient, pos, NullPosProvider, PosProvider,
};
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .context("failed to connect to database")?;

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    let mailer = auth::build_mailer()?;
    let base_url = std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());

    let encryption_key_hex =
        std::env::var("ENCRYPTION_KEY").context("ENCRYPTION_KEY must be set")?;
    let encryption_key = EncryptionKey::from_hex(&encryption_key_hex)
        .context("ENCRYPTION_KEY must be 64 hex chars (32 bytes)")?;

    let tcg_api_key = std::env::var("POKEMON_TCG_API_KEY").ok();
    let tcg_client = Arc::new(PokemonTcgClient::new(tcg_api_key));
    let valuation = app::build_valuation_service(pool.clone());
    catalogue::valuation::spawn_valuation_refresh_scheduler(Arc::clone(&valuation));

    let psa_api_key = std::env::var("PSA_API_KEY").ok();
    let grading = Arc::new(GradingService::new(pool.clone(), psa_api_key, 3600));

    let redis = match std::env::var("REDIS_URL") {
        Ok(url) => {
            let client = redis::Client::open(url.as_str())?;
            match redis::aio::ConnectionManager::new(client).await {
                Ok(conn) => {
                    tracing::info!("connected to Redis — velocity counters active");
                    Some(conn)
                }
                Err(e) => {
                    tracing::warn!("Redis unavailable: {e} — velocity counters disabled");
                    None
                }
            }
        }
        Err(_) => {
            tracing::info!("REDIS_URL not set — velocity counters will use Postgres fallback");
            None
        }
    };

    let provider: Arc<dyn PosProvider> = Arc::new(NullPosProvider);
    pos::reconcile::spawn_reconciliation_scheduler(pool.clone(), Arc::clone(&provider));

    let state = app::AppState { pool, mailer, base_url, encryption_key, tcg_client, grading, valuation, redis };

    let router = app::create_router()
        .merge(pos::reconcile::routes())
        .merge(pos::mapping::routes())
        .layer(Extension(Arc::clone(&provider)))
        .with_state(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, router).await?;
    Ok(())
}
