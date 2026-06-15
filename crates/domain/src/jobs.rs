use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a background import or export job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
}

/// Lightweight summary of an import or export job for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    pub workspace_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    /// For import: rows processed, imported, skipped.
    pub progress: Option<JobProgress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    CsvImport,
    CsvExport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    pub total_rows: u64,
    pub processed_rows: u64,
    pub imported_rows: u64,
    pub skipped_rows: u64,
}
