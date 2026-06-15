//! CSV export — filtered or full catalogue export as a background job.
//!
//! Flow:
//!   1. POST /catalogue/export   → queue job → job_id
//!   2. GET  /catalogue/export/:id  → poll status
//!   3. GET  /catalogue/export/:id/download  → stream the produced CSV

use std::path::PathBuf;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

use crate::{
    error::{ApiResult, AppError},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ExportFilters {
    pub set_code: Option<String>,
    pub condition: Option<String>,
    pub include_raw: Option<bool>,
    pub include_graded: Option<bool>,
    pub in_stock_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartExportRequest {
    pub workspace_id: Uuid,
    #[serde(default)]
    pub filters: ExportFilters,
}

#[derive(Debug, Serialize)]
pub struct StartExportResponse {
    pub job_id: Uuid,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ExportStatusResponse {
    pub job_id: Uuid,
    pub status: String,
    pub row_count: Option<i64>,
    pub download_ready: bool,
    pub error_message: Option<String>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /catalogue/export/template — blank CSV showing the export column layout.
pub async fn download_export_template() -> impl IntoResponse {
    // Header row matches the `ExportRow` struct field order (serde Serialize).
    let template = concat!(
        "printing_id,set_code,collector_number,name,condition_or_grade,",
        "quantity,acquisition_cost_usd,current_value_usd,",
        "grader,cert_number,verification_status,notes,created_at\r\n",
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            r#"attachment; filename="cardguard-export-template.csv""#,
        )
        .body(Body::from(template))
        .unwrap()
}

/// POST /catalogue/export
pub async fn start_export(
    State(state): State<AppState>,
    Json(req): Json<StartExportRequest>,
) -> ApiResult<impl IntoResponse> {
    let job_id = Uuid::new_v4();
    let filters_json = serde_json::to_value(&req.filters).map_err(anyhow::Error::from)?;

    sqlx::query(
        "INSERT INTO export_jobs (id, workspace_id, filters) VALUES ($1, $2, $3)",
    )
    .bind(job_id)
    .bind(req.workspace_id)
    .bind(filters_json)
    .execute(&state.db)
    .await?;

    {
        let state2 = state.clone();
        let workspace_id = req.workspace_id;
        let filters = req.filters;
        tokio::spawn(async move {
            if let Err(e) = run_export_job(state2, job_id, workspace_id, filters).await {
                tracing::error!(job_id = %job_id, "export job failed: {e:#}");
            }
        });
    }

    Ok((StatusCode::ACCEPTED, Json(StartExportResponse { job_id, status: "queued" })))
}

/// GET /catalogue/export/:job_id
pub async fn get_export_status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT id, status, row_count, download_path, error_message, completed_at
         FROM export_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("export job {job_id} not found")))?;

    let status: String = row.try_get("status").unwrap_or_default();
    let download_path: Option<String> = row.try_get("download_path").ok().flatten();

    Ok(Json(ExportStatusResponse {
        job_id: row.try_get("id").unwrap_or(job_id),
        download_ready: status == "done" && download_path.is_some(),
        status,
        row_count: row.try_get("row_count").ok().flatten(),
        error_message: row.try_get("error_message").ok().flatten(),
        completed_at: row.try_get("completed_at").ok().flatten(),
    }))
}

/// GET /catalogue/export/:job_id/download
pub async fn download_export(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    use sqlx::Row;

    let row = sqlx::query("SELECT status, download_path FROM export_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("export job {job_id} not found")))?;

    let status: String = row.try_get("status").unwrap_or_default();
    if status != "done" {
        return Err(AppError::BadRequest(format!(
            "export job is not done yet (status: {status})"
        )));
    }

    let path: Option<String> = row.try_get("download_path").ok().flatten();
    let path = path.ok_or_else(|| AppError::NotFound("no download file for this job".to_owned()))?;

    let bytes = fs::read(&path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("read export file: {e}")))?;

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"cardguard-export-{job_id}.csv\""),
        )
        .body(Body::from(bytes))
        .unwrap())
}

// ---------------------------------------------------------------------------
// Export job execution
// ---------------------------------------------------------------------------

/// One row in the export CSV — covers both raw and graded cards.
#[derive(Debug, Serialize)]
struct ExportRow {
    printing_id: String,
    set_code: String,
    collector_number: String,
    name: String,
    /// Condition for raw; grade label for graded.
    condition_or_grade: String,
    quantity: i64,
    acquisition_cost_usd: String,
    current_value_usd: String,
    grader: String,
    cert_number: String,
    verification_status: String,
    notes: String,
    created_at: String,
}

async fn run_export_job(
    state: AppState,
    job_id: Uuid,
    workspace_id: Uuid,
    filters: ExportFilters,
) -> anyhow::Result<()> {
    use sqlx::Row;

    sqlx::query("UPDATE export_jobs SET status = 'running' WHERE id = $1")
        .bind(job_id)
        .execute(&state.db)
        .await?;

    let include_raw = filters.include_raw.unwrap_or(true);
    let include_graded = filters.include_graded.unwrap_or(true);
    let in_stock_only = filters.in_stock_only.unwrap_or(false);

    let mut rows: Vec<ExportRow> = Vec::new();

    // Raw inventory items
    if include_raw {
        let mut q = String::from(
            "SELECT ii.printing_id, ii.condition, ii.quantity,
                    ii.acquisition_cost_cents, ii.notes,
                    ii.created_at::text as created_at,
                    p.set_code, p.collector_number, p.name,
                    v.price_cents
             FROM inventory_items ii
             JOIN printings p ON p.id = ii.printing_id
             LEFT JOIN valuations v ON v.printing_id = ii.printing_id
                                    AND v.source = 'tcgplayer'
                                    AND v.condition = ii.condition
             WHERE ii.workspace_id = $1",
        );

        let mut bind_idx = 2usize;
        let mut set_code_val: Option<String> = None;
        let mut condition_val: Option<String> = None;

        if let Some(ref sc) = filters.set_code {
            q.push_str(&format!(" AND p.set_code = ${bind_idx}"));
            set_code_val = Some(sc.clone());
            bind_idx += 1;
        }
        if let Some(ref cond) = filters.condition {
            q.push_str(&format!(" AND ii.condition = ${bind_idx}"));
            condition_val = Some(cond.clone());
            bind_idx += 1;
        }
        if in_stock_only {
            q.push_str(" AND ii.quantity > 0");
        }

        let _ = bind_idx; // silence unused warning

        let mut query = sqlx::query(&q).bind(workspace_id);
        if let Some(sc) = set_code_val {
            query = query.bind(sc);
        }
        if let Some(cond) = condition_val {
            query = query.bind(cond);
        }

        for row in query.fetch_all(&state.db).await? {
            rows.push(ExportRow {
                printing_id: row.try_get("printing_id").unwrap_or_default(),
                set_code: row.try_get("set_code").unwrap_or_default(),
                collector_number: row.try_get("collector_number").unwrap_or_default(),
                name: row.try_get("name").unwrap_or_default(),
                condition_or_grade: row.try_get("condition").unwrap_or_default(),
                quantity: row.try_get::<i32, _>("quantity").unwrap_or(0) as i64,
                acquisition_cost_usd: cents_to_usd(row.try_get("acquisition_cost_cents").ok()),
                current_value_usd: cents_to_usd(row.try_get("price_cents").ok()),
                grader: String::new(),
                cert_number: String::new(),
                verification_status: String::new(),
                notes: row.try_get("notes").ok().flatten().unwrap_or_default(),
                created_at: row.try_get("created_at").unwrap_or_default(),
            });
        }
    }

    // Graded card instances
    if include_graded {
        let mut q = String::from(
            "SELECT ci.printing_id, ci.grader, ci.cert_number, ci.grade,
                    ci.verification_status, ci.acquisition_cost_cents,
                    ci.notes, ci.created_at::text as created_at,
                    p.set_code, p.collector_number, p.name,
                    v.price_cents
             FROM card_instances ci
             JOIN printings p ON p.id = ci.printing_id
             LEFT JOIN valuations v ON v.printing_id = ci.printing_id
                                    AND v.source = 'pricecharting'
                                    AND v.condition IS NULL
             WHERE ci.workspace_id = $1",
        );

        let mut bind_idx = 2usize;
        let mut set_code_val: Option<String> = None;

        if let Some(ref sc) = filters.set_code {
            q.push_str(&format!(" AND p.set_code = ${bind_idx}"));
            set_code_val = Some(sc.clone());
            bind_idx += 1;
        }

        let _ = bind_idx;

        let mut query = sqlx::query(&q).bind(workspace_id);
        if let Some(sc) = set_code_val {
            query = query.bind(sc);
        }

        for row in query.fetch_all(&state.db).await? {
            rows.push(ExportRow {
                printing_id: row.try_get("printing_id").unwrap_or_default(),
                set_code: row.try_get("set_code").unwrap_or_default(),
                collector_number: row.try_get("collector_number").unwrap_or_default(),
                name: row.try_get("name").unwrap_or_default(),
                condition_or_grade: row.try_get("grade").unwrap_or_default(),
                quantity: 1,
                acquisition_cost_usd: cents_to_usd(row.try_get("acquisition_cost_cents").ok()),
                current_value_usd: cents_to_usd(row.try_get("price_cents").ok()),
                grader: row.try_get("grader").unwrap_or_default(),
                cert_number: row.try_get("cert_number").unwrap_or_default(),
                verification_status: row.try_get("verification_status").unwrap_or_default(),
                notes: row.try_get("notes").ok().flatten().unwrap_or_default(),
                created_at: row.try_get("created_at").unwrap_or_default(),
            });
        }
    }

    // Serialize to CSV
    let mut wtr = csv::Writer::from_writer(vec![]);
    for row in &rows {
        wtr.serialize(row)?;
    }
    let csv_bytes = wtr.into_inner()?;
    let row_count = rows.len() as i64;

    // Write to disk
    let out_path = export_file_path(&state.config.export_dir, job_id);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(&out_path, &csv_bytes).await?;

    // Mark done
    sqlx::query(
        "UPDATE export_jobs
         SET status='done', row_count=$1, download_path=$2, completed_at=now()
         WHERE id=$3",
    )
    .bind(row_count)
    .bind(out_path.to_str().unwrap_or(""))
    .bind(job_id)
    .execute(&state.db)
    .await?;

    tracing::info!(job_id = %job_id, row_count, "CSV export complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn export_file_path(base_dir: &str, job_id: Uuid) -> PathBuf {
    PathBuf::from(base_dir)
        .join("exports")
        .join(format!("{job_id}.csv"))
}

fn cents_to_usd(cents: Option<i64>) -> String {
    match cents {
        Some(c) => format!("{:.2}", c as f64 / 100.0),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_to_usd_conversion() {
        assert_eq!(cents_to_usd(Some(1000)), "10.00");
        assert_eq!(cents_to_usd(Some(150)), "1.50");
        assert_eq!(cents_to_usd(Some(0)), "0.00");
        assert_eq!(cents_to_usd(None), "");
    }

    #[test]
    fn export_filters_defaults() {
        let f = ExportFilters::default();
        assert!(f.include_raw.unwrap_or(true));
        assert!(f.include_graded.unwrap_or(true));
        assert!(!f.in_stock_only.unwrap_or(false));
    }

    #[test]
    fn export_file_path_structure() {
        let id = Uuid::nil();
        let p = export_file_path("/tmp", id);
        assert!(p.to_str().unwrap().contains("exports"));
        assert!(p.to_str().unwrap().ends_with(".csv"));
    }
}
