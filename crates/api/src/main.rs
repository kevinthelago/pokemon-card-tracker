use anyhow::Context;
use axum::Extension;
use cardguard_api::{app, auth, crypto::EncryptionKey, pos, NullPosProvider, PosProvider};
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
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
    let base_url =
        std::env::var("APP_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".into());

    let encryption_key_hex =
        std::env::var("ENCRYPTION_KEY").context("ENCRYPTION_KEY must be set")?;
    let encryption_key = EncryptionKey::from_hex(&encryption_key_hex)
        .context("ENCRYPTION_KEY must be 64 hex chars (32 bytes)")?;

    let provider: Arc<dyn PosProvider> = Arc::new(NullPosProvider);

    pos::reconcile::spawn_reconciliation_scheduler(pool.clone(), Arc::clone(&provider));

    let state = app::AppState { pool, mailer, base_url, encryption_key };

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
