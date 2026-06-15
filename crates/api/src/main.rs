use axum::{routing::get, Router};
use cardguard_api::{pos, AppState, NullPosProvider};
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("../../migrations").run(&pool).await?;

    let workspace_id = uuid::Uuid::parse_str(
        &std::env::var("WORKSPACE_ID").unwrap_or_else(|_| uuid::Uuid::nil().to_string()),
    )?;

    let provider: Arc<dyn cardguard_api::PosProvider> = Arc::new(NullPosProvider);

    // Spawn scheduled reconciliation (hourly)
    pos::reconcile::spawn_reconciliation_scheduler(pool.clone(), Arc::clone(&provider));

    let state = AppState {
        pool,
        workspace_id,
        pos_provider: provider,
    };

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(pos::reconcile::routes())
        .merge(pos::mapping::routes())
        .with_state(state);

    let addr: SocketAddr = "0.0.0.0:3001".parse()?;
    tracing::info!("API listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
