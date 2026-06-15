use axum::{routing::get, Router};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::{catalogue, fraud, workspace};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub mailer: AsyncSmtpTransport<Tokio1Executor>,
    pub base_url: String,
}

/// Returns a `Router<AppState>` without consuming state so callers can
/// merge additional routes before calling `.with_state()`.
pub fn create_router() -> Router<AppState> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api", workspace::routes().merge(fraud::stolen::routes()))
        .nest("/api", fraud::routes::routes())
        .nest("/api/catalogue", catalogue::csv_routes())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}
