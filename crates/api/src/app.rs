use axum::{http::Method, routing::get, Router};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::grading::GradingService;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub grading: Arc<GradingService>,
}

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_origin(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        // Grading verification (verify-graded-card stream)
        .nest("/api", crate::grading::routes::router())
        // TODO(catalogue-a-card):  .nest("/api", crate::catalogue::routes::router())
        // TODO(manage-inventory):  .nest("/api", crate::inventory::routes::router())
        // TODO(track-values):      .nest("/api", crate::valuation::routes::router())
        // TODO(connect-pos):       .nest("/api", crate::pos::routes::router())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}
