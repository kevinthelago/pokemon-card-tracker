use axum::{routing::get, Router};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::catalogue::valuation::ValuationService;
use crate::integrations::pricecharting::PriceChartingClient;
use crate::{
    catalogue, crypto::EncryptionKey, fraud, grading::GradingService,
    integrations::pokemontcg::PokemonTcgClient, pos, workspace,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub mailer: AsyncSmtpTransport<Tokio1Executor>,
    pub base_url: String,
    pub encryption_key: EncryptionKey,
    pub tcg_client: Arc<PokemonTcgClient>,
    pub grading: Arc<GradingService>,
    pub valuation: Arc<ValuationService>,
    /// Present when REDIS_URL is set; used by scalper velocity counters.
    pub redis: Option<redis::aio::ConnectionManager>,
}

impl AppState {
    pub fn valuation_service(&self) -> &ValuationService {
        &self.valuation
    }
}

pub fn build_valuation_service(pool: PgPool) -> Arc<ValuationService> {
    let api_key = std::env::var("PRICECHARTING_API_KEY").unwrap_or_default();
    Arc::new(ValuationService::new(
        pool,
        PriceChartingClient::new(api_key),
    ))
}

pub fn create_router() -> Router<AppState> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest(
            "/api",
            workspace::routes()
                .merge(fraud::stolen::routes())
                .merge(catalogue::routes()),
        )
        .nest("/api", fraud::routes::routes())
        .nest("/api", catalogue::routes::router())
        .nest("/api", crate::grading::routes::router())
        .nest("/api", fraud::detect_scalpers::routes())
        .nest("/api/catalogue", catalogue::csv_routes())
        .nest("/api", catalogue::valuation_routes::routes())
        .merge(pos::connect::routes())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}
