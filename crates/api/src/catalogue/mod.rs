pub mod csv_export;
pub mod csv_import;

use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

/// Catalogue sub-router. State is provided by the parent router.
pub fn routes() -> Router<AppState> {
    Router::new()
        // CSV import
        .route("/import/template", get(csv_import::download_template))
        .route("/import", post(csv_import::start_import))
        .route("/import/{job_id}", get(csv_import::get_import_status))
        .route("/import/{job_id}/confirm", post(csv_import::confirm_import))
        .route(
            "/import/{job_id}/errors",
            get(csv_import::download_error_report),
        )
        // CSV export
        .route("/export/template", get(csv_export::download_export_template))
        .route("/export", post(csv_export::start_export))
        .route("/export/{job_id}", get(csv_export::get_export_status))
        .route(
            "/export/{job_id}/download",
            get(csv_export::download_export),
        )
}
