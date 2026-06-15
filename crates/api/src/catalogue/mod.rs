//! Catalogue module.
//!
//! Streams and their files:
//!   catalogue-a-card  → identity.rs, items.rs, routes.rs  (THIS stream)
//!   manage-inventory  → inventory.rs  (fill crates/api/src/catalogue/inventory.rs)
//!   track-values      → valuation.rs  (fill crates/api/src/catalogue/valuation.rs)
//!   csv-import-export → csv_import.rs, csv_export.rs

pub mod csv_export;
pub mod csv_import;
pub mod identity;
pub mod inventory;
pub mod items;
pub mod routes;
pub mod valuation;

use axum::{
    routing::{get, post},
    Router,
};

use crate::app::AppState;

/// CSV import / export routes.  Mount with:
///   `.nest("/api/catalogue", catalogue::csv_routes())`
pub fn csv_routes() -> Router<AppState> {
    Router::new()
        // Import
        .route("/import/template", get(csv_import::download_template))
        .route("/import", post(csv_import::start_import))
        .route("/import/{job_id}", get(csv_import::get_import_status))
        .route("/import/{job_id}/confirm", post(csv_import::confirm_import))
        .route("/import/{job_id}/errors", get(csv_import::download_error_report))
        // Export
        .route("/export/template", get(csv_export::download_export_template))
        .route("/export", post(csv_export::start_export))
        .route("/export/{job_id}", get(csv_export::get_export_status))
        .route("/export/{job_id}/download", get(csv_export::download_export))
}
