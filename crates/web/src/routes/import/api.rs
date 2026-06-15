//! HTTP client helpers — available in CSR (WASM) builds only.
//! SSR builds use Leptos server functions directly.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub const API_BASE: &str = "/api";

// ---------------------------------------------------------------------------
// Shared types (used by both SSR and CSR)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ColumnMap {
    pub set_code: Option<String>,
    pub collector_number: Option<String>,
    pub printing_id: Option<String>,
    pub name: Option<String>,
    pub condition: Option<String>,
    pub quantity: Option<String>,
    pub acquisition_cost: Option<String>,
    pub grader: Option<String>,
    pub cert_number: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetectedColumns {
    pub headers: Vec<String>,
    pub mapping: ColumnMap,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StartImportResponse {
    pub job_id: Uuid,
    pub status: String,
    pub detected_columns: DetectedColumns,
    pub preview_rows: Vec<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImportStatus {
    pub job_id: Uuid,
    pub status: String,
    pub total_rows: Option<i64>,
    pub processed_rows: i64,
    pub imported_rows: i64,
    pub skipped_rows: i64,
    pub error_message: Option<String>,
    pub has_error_report: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportStatus {
    pub job_id: Uuid,
    pub status: String,
    pub row_count: Option<i64>,
    pub download_ready: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ExportFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_raw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_graded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock_only: Option<bool>,
}

// ---------------------------------------------------------------------------
// URL helpers (available everywhere)
// ---------------------------------------------------------------------------

pub fn import_error_report_url(job_id: Uuid) -> String {
    format!("{API_BASE}/catalogue/import/{job_id}/errors")
}

pub fn export_download_url(job_id: Uuid) -> String {
    format!("{API_BASE}/catalogue/export/{job_id}/download")
}

pub fn import_template_url() -> String {
    format!("{API_BASE}/catalogue/import/template")
}

// ---------------------------------------------------------------------------
// CSR-only fetch helpers (WASM)
// ---------------------------------------------------------------------------

#[cfg(feature = "csr")]
pub async fn get_import_status(job_id: Uuid) -> Result<ImportStatus, String> {
    let url = format!("{API_BASE}/catalogue/import/{job_id}");
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<ImportStatus>().await.map_err(|e| e.to_string())
}

#[cfg(feature = "csr")]
pub async fn get_export_status(job_id: Uuid) -> Result<ExportStatus, String> {
    let url = format!("{API_BASE}/catalogue/export/{job_id}");
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<ExportStatus>().await.map_err(|e| e.to_string())
}
