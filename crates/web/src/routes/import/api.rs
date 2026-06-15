use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Clone)]
pub struct StartImportResponse {
    pub job_id: Uuid,
    pub status: String,
    pub total_rows: i64,
    pub error_count: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ImportStatusResponse {
    pub job_id: Uuid,
    pub status: String,
    pub filename: Option<String>,
    pub total_rows: Option<i64>,
    pub processed_rows: i64,
    pub imported_rows: i64,
    pub skipped_rows: i64,
    pub error_count: i64,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ExportFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_raw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_graded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_stock_only: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct StartExportRequest {
    pub workspace_id: Uuid,
    pub filters: ExportFilters,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StartExportResponse {
    pub job_id: Uuid,
    pub status: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExportStatusResponse {
    pub job_id: Uuid,
    pub status: String,
    pub row_count: Option<i64>,
    pub download_ready: bool,
    pub error_message: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}
