use axum::{routing::get, Router};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::{
    catalogue, crypto::EncryptionKey, fraud,
    grading::GradingService,
    integrations::pokemontcg::PokemonTcgClient,
    pos, workspace,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub mailer: AsyncSmtpTransport<Tokio1Executor>,
    pub base_url: String,
    pub encryption_key: EncryptionKey,
    pub tcg_client: Arc<PokemonTcgClient>,
    pub grading: Arc<GradingService>,
}

pub fn create_router() -> Router<AppState> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api", workspace::routes().merge(fraud::stolen::routes()))
        .nest("/api", fraud::routes::routes())
        .nest("/api", catalogue::routes::router())
        .nest("/api", crate::grading::routes::router())
        .nest("/api/catalogue", catalogue::csv_routes())
        .merge(pos::connect::routes())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}
