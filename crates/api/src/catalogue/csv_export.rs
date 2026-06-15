//! CSV export — filtered or full catalogue export as a background job.
//!
//! Flow:
//!   1. GET  /catalogue/export/template         → column-layout template CSV
//!   2. POST /catalogue/export                  → queue job → job_id
//!   3. GET  /catalogue/export/:id              → poll status
//!   4. GET  /catalogue/export/:id/download     → stream the produced CSV

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

use crate::{app::AppState, error::AppError};

type ApiResult<T> = Result<T, AppError>;

fn export_dir() -> String {
    std::env::var("EXPORT_DIR").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("cardguard-exports")
            .to_string_lossy()
            .into_owned()
    })
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ExportFilters {
    pub set_id: Option<String>,
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

/// GET /catalogue/export/template
pub async fn download_export_template() -> impl IntoResponse {
    let template = concat!(
        "printing_id,set_id,number,name,condition_or_grade,quantity,",
        "acquisition_cost_usd,current_value_usd,",
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

    sqlx::query("INSERT INTO export_jobs (id, workspace_id, filters) VALUES ($1, $2, $3)")
        .bind(job_id)
        .bind(req.workspace_id)
        .bind(filters_json)
        .execute(&state.pool)
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

    Ok((
        StatusCode::ACCEPTED,
        Json(StartExportResponse {
            job_id,
            status: "queued",
        }),
    ))
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
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

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
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let status: String = row.try_get("status").unwrap_or_default();
    if status != "done" {
        return Err(AppError::BadRequest(format!(
            "export job is not done yet (status: {status})"
        )));
    }

    let path: Option<String> = row.try_get("download_path").ok().flatten();
    let path = path.ok_or(AppError::NotFound)?;

    let bytes = fs::read(&path).await.map_err(anyhow::Error::from)?;

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
// Export job
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ExportRow {
    printing_id: String,
    set_id: String,
    number: String,
    name: String,
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
        .execute(&state.pool)
        .await?;

    let include_raw = filters.include_raw.unwrap_or(true);
    let include_graded = filters.include_graded.unwrap_or(true);
    let in_stock_only = filters.in_stock_only.unwrap_or(false);

    let mut rows: Vec<ExportRow> = Vec::new();

    // Raw inventory items
    if include_raw {
        let mut q = String::from(
            "SELECT ii.id, ii.printing_id::text, ii.condition, ii.quantity,
                    ii.acquisition_cost_cents, ii.notes,
                    ii.created_at::text,
                    p.set_id, p.number, p.name,
                    v.market_price_cents
             FROM inventory_items ii
             LEFT JOIN printings p ON p.id = ii.printing_id
             LEFT JOIN valuations v ON v.inventory_item_id = ii.id
             WHERE ii.workspace_id = $1",
        );

        let mut bind_idx = 2usize;
        let mut set_id_val: Option<String> = None;
        let mut cond_val: Option<String> = None;

        if let Some(ref sc) = filters.set_id {
            q.push_str(&format!(" AND p.set_id = ${bind_idx}"));
            set_id_val = Some(sc.clone());
            bind_idx += 1;
        }
        if let Some(ref cond) = filters.condition {
            q.push_str(&format!(" AND ii.condition = ${bind_idx}"));
            cond_val = Some(cond.clone());
            bind_idx += 1;
        }
        if in_stock_only {
            q.push_str(" AND ii.quantity > 0");
        }
        let _ = bind_idx;

        let mut query = sqlx::query(&q).bind(workspace_id);
        if let Some(sc) = set_id_val {
            query = query.bind(sc);
        }
        if let Some(c) = cond_val {
            query = query.bind(c);
        }

        for row in query.fetch_all(&state.pool).await? {
            rows.push(ExportRow {
                printing_id: row
                    .try_get::<Option<String>, _>("printing_id")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                set_id: row.try_get("set_id").ok().flatten().unwrap_or_default(),
                number: row.try_get("number").ok().flatten().unwrap_or_default(),
                name: row.try_get("name").ok().flatten().unwrap_or_default(),
                condition_or_grade: row.try_get("condition").ok().flatten().unwrap_or_default(),
                quantity: row.try_get::<i32, _>("quantity").unwrap_or(0) as i64,
                acquisition_cost_usd: cents_to_usd(
                    row.try_get::<Option<i32>, _>("acquisition_cost_cents")
                        .ok()
                        .flatten()
                        .map(|c| c as i64),
                ),
                current_value_usd: cents_to_usd(
                    row.try_get::<Option<i32>, _>("market_price_cents")
                        .ok()
                        .flatten()
                        .map(|c| c as i64),
                ),
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
            "SELECT ci.id, ci.printing_id::text, ci.grader, ci.cert_number,
                    ci.grade, ci.verification_status,
                    ci.acquisition_cost_cents, ci.notes,
                    ci.created_at::text,
                    p.set_id, p.number, p.name,
                    v.market_price_cents
             FROM card_instances ci
             LEFT JOIN printings p ON p.id = ci.printing_id
             LEFT JOIN valuations v ON v.card_instance_id = ci.id
             WHERE ci.workspace_id = $1",
        );

        let mut bind_idx = 2usize;
        let mut set_id_val: Option<String> = None;

        if let Some(ref sc) = filters.set_id {
            q.push_str(&format!(" AND p.set_id = ${bind_idx}"));
            set_id_val = Some(sc.clone());
            bind_idx += 1;
        }
        let _ = bind_idx;

        let mut query = sqlx::query(&q).bind(workspace_id);
        if let Some(sc) = set_id_val {
            query = query.bind(sc);
        }

        for row in query.fetch_all(&state.pool).await? {
            rows.push(ExportRow {
                printing_id: row
                    .try_get::<Option<String>, _>("printing_id")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                set_id: row.try_get("set_id").ok().flatten().unwrap_or_default(),
                number: row.try_get("number").ok().flatten().unwrap_or_default(),
                name: row.try_get("name").ok().flatten().unwrap_or_default(),
                condition_or_grade: row
                    .try_get::<Option<String>, _>("grade")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                quantity: 1,
                acquisition_cost_usd: cents_to_usd(
                    row.try_get::<Option<i32>, _>("acquisition_cost_cents")
                        .ok()
                        .flatten()
                        .map(|c| c as i64),
                ),
                current_value_usd: cents_to_usd(
                    row.try_get::<Option<i32>, _>("market_price_cents")
                        .ok()
                        .flatten()
                        .map(|c| c as i64),
                ),
                grader: row.try_get("grader").unwrap_or_default(),
                cert_number: row.try_get("cert_number").unwrap_or_default(),
                verification_status: row.try_get("verification_status").unwrap_or_default(),
                notes: row.try_get("notes").ok().flatten().unwrap_or_default(),
                created_at: row.try_get("created_at").unwrap_or_default(),
            });
        }
    }

    let mut wtr = csv::Writer::from_writer(vec![]);
    for row in &rows {
        wtr.serialize(row)?;
    }
    let csv_bytes = wtr.into_inner()?;
    let row_count = rows.len() as i64;

    let dir = export_dir();
    let out_path = export_file_path(&dir, job_id);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(&out_path, &csv_bytes).await?;

    sqlx::query(
        "UPDATE export_jobs
         SET status='done', row_count=$1, download_path=$2, completed_at=now()
         WHERE id=$3",
    )
    .bind(row_count)
    .bind(out_path.to_str().unwrap_or(""))
    .bind(job_id)
    .execute(&state.pool)
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
