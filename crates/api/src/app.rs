use axum::{Router, routing::get};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::workspace;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub mailer: AsyncSmtpTransport<Tokio1Executor>,
    pub base_url: String,
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api", workspace::routes())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
