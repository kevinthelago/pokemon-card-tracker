pub mod authz;
pub mod invites;
pub mod routes;

#[cfg(test)]
mod tests;

use axum::Router;

use crate::{app::AppState, auth};

/// All workspace-related + auth routes, merged for the app router.
pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(invites::routes())
        .merge(routes::router())
        .merge(auth::routes::router())
}
