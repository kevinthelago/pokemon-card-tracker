pub mod invites;

use crate::app::AppState;
use axum::Router;

pub fn routes() -> Router<AppState> {
    Router::new().merge(invites::routes())
}
