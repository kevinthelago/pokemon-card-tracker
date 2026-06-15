//! CSV import — multipart upload, background job, column mapping, dedup, error reporting.
//!
//! Flow:
//!   1. GET  /catalogue/import/template     → blank template CSV
//!   2. POST /catalogue/import (multipart)  → job_id + detected columns + sample rows
//!   3. POST /catalogue/import/:id/confirm  → start job with finalised column map
//!   4. GET  /catalogue/import/:id          → poll status / progress
//!   5. GET  /catalogue/import/:id/errors   → download CSV error report for skipped rows

use std::{collections::HashMap, path::PathBuf};

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{app::AppState, error::AppError};

type ApiResult<T> = Result<T, AppError>;

// ---------------------------------------------------------------------------
// Domain helpers (inlined — no separate domain crate on this branch yet)
// ---------------------------------------------------------------------------

fn condition_from_str(s: &str) -> Option<&'static str> {
    match s.trim().to_uppercase().replace([' ', '-', '_'], "").as_str() {
        "M" | "MINT" => Some("mint"),
        "NM" | "NEARMINT" => Some("near_mint"),
        "LP" | "LIGHTLYPLAYED" | "EX" | "EXCELLENT" => Some("lightly_played"),
        "MP" | "MODERATELYPLAYED" | "VG" | "VERYGOOD" | "GD" | "GOOD" => {
            Some("moderately_played")
        }
        "HP" | "HEAVILYPLAYED" | "PO" | "POOR" | "FR" | "FAIR" => Some("heavily_played"),
        "D" | "DAMAGED" => Some("damaged"),
        _ => None,
    }
}

fn grader_from_str(s: &str) -> Option<&'static str> {
    match s.trim().to_uppercase().as_str() {
        "PSA" => Some("PSA"),
        "CGC" => Some("CGC"),
        "BGS" | "BECKETT" => Some("BGS"),
        _ => None,
    }
}

fn export_dir() -> String {
    std::env::var("EXPORT_DIR").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("cardguard-exports")
            .to_string_lossy()
            .into_owned()
    })
}

// ---------------------------------------------------------------------------
// Public request / response types
// ---------------------------------------------------------------------------

/// Mapping from logical field → CSV header name in the uploaded file.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ColumnMap {
    pub set_id: Option<String>,
    pub number: Option<String>,
    pub name: Option<String>,
    pub condition: Option<String>,
    pub quantity: Option<String>,
    pub acquisition_cost: Option<String>,
    pub grader: Option<String>,
    pub cert_number: Option<String>,
    pub notes: Option<String>,
}

impl ColumnMap {
    pub fn auto_detect(headers: &[String]) -> Self {
        let lower: Vec<String> = headers
            .iter()
            .map(|h| h.to_lowercase().replace([' ', '-'], "_"))
            .collect();

        let find = |candidates: &[&str]| -> Option<String> {
            for c in candidates {
                if let Some(pos) = lower.iter().position(|h| h == c) {
                    return Some(headers[pos].clone());
                }
            }
            None
        };

        ColumnMap {
            set_id: find(&["set_id", "set_code", "set", "set_name"]),
            number: find(&["number", "collector_number", "card_number", "num"]),
            name: find(&["name", "card_name"]),
            condition: find(&["condition", "cond", "grade"]),
            quantity: find(&["quantity", "qty", "count", "amount"]),
            acquisition_cost: find(&["acquisition_cost", "cost", "price", "paid"]),
            grader: find(&["grader", "grading_company"]),
            cert_number: find(&["cert_number", "cert_no", "cert", "certification_number"]),
            notes: find(&["notes", "note", "comments"]),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.condition.as_ref().map_or(true, |s| s.is_empty()) {
            return Err("'condition' column mapping is required".to_owned());
        }
        if self.quantity.as_ref().map_or(true, |s| s.is_empty()) {
            return Err("'quantity' column mapping is required".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct StartImportResponse {
    pub job_id: Uuid,
    pub status: String,
    pub detected_columns: DetectedColumns,
    pub preview_rows: Vec<HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
pub struct DetectedColumns {
    pub headers: Vec<String>,
    pub mapping: ColumnMap,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportStatusResponse {
    pub job_id: Uuid,
    pub status: String,
    pub total_rows: Option<i64>,
    pub processed_rows: i64,
    pub imported_rows: i64,
    pub skipped_rows: i64,
    pub error_message: Option<String>,
    pub has_error_report: bool,
    pub completed_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmImportRequest {
    pub column_map: ColumnMap,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /catalogue/import/template
pub async fn download_template() -> impl IntoResponse {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            r#"attachment; filename="cardguard-import-template.csv""#,
        )
        .body(Body::from(generate_import_template()))
        .unwrap()
}

/// POST /catalogue/import
pub async fn start_import(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
    let mut csv_bytes: Option<Vec<u8>> = None;
    let mut workspace_id: Option<Uuid> = None;
    let mut provided_map: Option<ColumnMap> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart error: {e}")))?
    {
        match field.name() {
            Some("file") => {
                filename = field.file_name().map(ToOwned::to_owned);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("read file: {e}")))?;
                csv_bytes = Some(bytes.to_vec());
            }
            Some("workspace_id") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("workspace_id: {e}")))?;
                workspace_id = Some(Uuid::parse_str(&text).map_err(|_| {
                    AppError::BadRequest("invalid workspace_id UUID".to_owned())
                })?);
            }
            Some("column_map") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("column_map: {e}")))?;
                provided_map = Some(serde_json::from_str(&text).map_err(|e| {
                    AppError::BadRequest(format!("invalid column_map JSON: {e}"))
                })?);
            }
            _ => {}
        }
    }

    let raw =
        csv_bytes.ok_or_else(|| AppError::BadRequest("no file field in upload".to_owned()))?;
    let workspace_id =
        workspace_id.ok_or_else(|| AppError::BadRequest("missing workspace_id".to_owned()))?;

    if raw.is_empty() {
        return Err(AppError::BadRequest("uploaded CSV file is empty".to_owned()));
    }

    let raw = strip_bom(&raw);

    let csv_text = std::str::from_utf8(raw).map_err(|_| {
        AppError::BadRequest(
            "File is not valid UTF-8. Please save as UTF-8 and re-upload.".to_owned(),
        )
    })?;

    let (headers, preview_rows) = parse_preview(csv_text)
        .map_err(|e| AppError::BadRequest(format!("CSV parse error: {e}")))?;

    if headers.is_empty() {
        return Err(AppError::BadRequest("CSV has no header row".to_owned()));
    }

    let mapping = provided_map.unwrap_or_else(|| ColumnMap::auto_detect(&headers));

    let mut warnings = Vec::new();
    if let Err(e) = mapping.validate() {
        warnings.push(e);
    }

    let job_id = Uuid::new_v4();
    let dir = export_dir();
    let upload_path = upload_file_path(&dir, job_id);
    if let Some(parent) = upload_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(anyhow::Error::from)?;
    }
    {
        let mut f = fs::File::create(&upload_path)
            .await
            .map_err(anyhow::Error::from)?;
        f.write_all(raw).await.map_err(anyhow::Error::from)?;
    }

    sqlx::query(
        "INSERT INTO import_jobs (id, workspace_id, status, filename) VALUES ($1, $2, 'queued', $3)",
    )
    .bind(job_id)
    .bind(workspace_id)
    .bind(filename.as_deref())
    .execute(&state.pool)
    .await?;

    if warnings.is_empty() {
        let state2 = state.clone();
        let map = mapping.clone();
        let path = upload_path.clone();
        tokio::spawn(async move {
            if let Err(e) = run_import_job(state2, job_id, path, map).await {
                tracing::error!(job_id = %job_id, "import job failed: {e:#}");
            }
        });
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(StartImportResponse {
            job_id,
            status: "queued".to_owned(),
            detected_columns: DetectedColumns {
                headers,
                mapping,
                warnings,
            },
            preview_rows,
        }),
    ))
}

/// POST /catalogue/import/:job_id/confirm
pub async fn confirm_import(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(body): Json<ConfirmImportRequest>,
) -> ApiResult<impl IntoResponse> {
    use sqlx::Row;

    let row = sqlx::query("SELECT status FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let status: String = row.try_get("status").unwrap_or_default();
    if status != "queued" {
        return Err(AppError::BadRequest(format!(
            "job is already in state '{status}'; cannot re-confirm"
        )));
    }

    body.column_map
        .validate()
        .map_err(AppError::BadRequest)?;

    let dir = export_dir();
    let upload_path = upload_file_path(&dir, job_id);

    tokio::spawn(async move {
        if let Err(e) = run_import_job(state, job_id, upload_path, body.column_map).await {
            tracing::error!(job_id = %job_id, "import job failed: {e:#}");
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "job_id": job_id, "status": "running" })),
    ))
}

/// GET /catalogue/import/:job_id
pub async fn get_import_status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    use sqlx::Row;

    let row = sqlx::query(
        "SELECT id, status, total_rows, processed_rows, imported_rows,
                skipped_rows, error_message, error_report, completed_at
         FROM import_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(ImportStatusResponse {
        job_id: row.try_get::<Uuid, _>("id").unwrap_or(job_id),
        status: row.try_get("status").unwrap_or_default(),
        total_rows: row.try_get("total_rows").ok(),
        processed_rows: row.try_get("processed_rows").unwrap_or(0),
        imported_rows: row.try_get("imported_rows").unwrap_or(0),
        skipped_rows: row.try_get("skipped_rows").unwrap_or(0),
        error_message: row.try_get("error_message").ok().flatten(),
        has_error_report: row
            .try_get::<Option<serde_json::Value>, _>("error_report")
            .ok()
            .flatten()
            .is_some(),
        completed_at: row.try_get("completed_at").ok().flatten(),
    }))
}

/// GET /catalogue/import/:job_id/errors
pub async fn download_error_report(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    use sqlx::Row;

    let row = sqlx::query("SELECT error_report FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let report: Option<serde_json::Value> = row.try_get("error_report").ok().flatten();
    let report = report.ok_or(AppError::NotFound)?;

    let errors: Vec<ErrorRow> = serde_json::from_value(report).map_err(anyhow::Error::from)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    for err in &errors {
        wtr.serialize(err).map_err(anyhow::Error::from)?;
    }
    let csv_bytes = wtr.into_inner().map_err(anyhow::Error::from)?;

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"import-errors-{job_id}.csv\""),
        )
        .body(Body::from(csv_bytes))
        .unwrap())
}

// ---------------------------------------------------------------------------
// Background job
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct ErrorRow {
    row_number: u64,
    raw_data: String,
    field: String,
    reason: String,
}

async fn run_import_job(
    state: AppState,
    job_id: Uuid,
    path: PathBuf,
    map: ColumnMap,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE import_jobs SET status = 'running' WHERE id = $1")
        .bind(job_id)
        .execute(&state.pool)
        .await?;

    use sqlx::Row;
    let workspace_id: Uuid = sqlx::query("SELECT workspace_id FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&state.pool)
        .await?
        .try_get("workspace_id")?;

    let raw = fs::read(&path).await?;
    let raw = strip_bom(&raw);
    let csv_text = std::str::from_utf8(raw)?;

    let condition_col = map.condition.unwrap_or_default();
    let quantity_col = map.quantity.unwrap_or_default();
    let cost_col = map.acquisition_cost;
    let grader_col = map.grader;
    let cert_col = map.cert_number;
    let notes_col = map.notes;
    let set_col = map.set_id;
    let num_col = map.number;

    let total_rows: u64 = {
        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(csv_text.as_bytes());
        rdr.records().count() as u64
    };

    sqlx::query("UPDATE import_jobs SET total_rows = $1 WHERE id = $2")
        .bind(total_rows as i64)
        .bind(job_id)
        .execute(&state.pool)
        .await?;

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(csv_text.as_bytes());

    let headers: Vec<String> = rdr.headers()?.iter().map(ToOwned::to_owned).collect();

    let mut errors: Vec<ErrorRow> = Vec::new();
    let mut imported: u64 = 0;
    let mut skipped: u64 = 0;
    let mut processed: u64 = 0;

    for (row_idx, result) in rdr.records().enumerate() {
        processed += 1;
        let row_number = (row_idx + 2) as u64;

        let record = match result {
            Ok(r) => r,
            Err(e) => {
                errors.push(ErrorRow {
                    row_number,
                    raw_data: String::new(),
                    field: "row".to_owned(),
                    reason: format!("CSV parse error: {e}"),
                });
                skipped += 1;
                continue;
            }
        };

        let raw_data = record.iter().collect::<Vec<_>>().join(",");

        let get = |col: &str| -> &str {
            headers
                .iter()
                .position(|h| h == col)
                .and_then(|i| record.get(i))
                .unwrap_or("")
                .trim()
        };

        // Resolve printing identity (UUID) via set_id + number lookup
        let printing_id: Option<Uuid> = if let (Some(ref sc), Some(ref nc)) =
            (set_col.as_ref(), num_col.as_ref())
        {
            let set = get(sc);
            let num = get(nc);
            if set.is_empty() || num.is_empty() {
                None
            } else {
                match sqlx::query(
                    "SELECT id FROM printings WHERE set_id = $1 AND number = $2 LIMIT 1",
                )
                .bind(set)
                .bind(num)
                .fetch_optional(&state.pool)
                .await
                {
                    Ok(Some(row)) => row.try_get::<Uuid, _>("id").ok(),
                    Ok(None) => {
                        errors.push(ErrorRow {
                            row_number,
                            raw_data: raw_data.clone(),
                            field: "set_id/number".to_owned(),
                            reason: format!("No printing found for set={set} number={num}"),
                        });
                        skipped += 1;
                        continue;
                    }
                    Err(e) => {
                        errors.push(ErrorRow {
                            row_number,
                            raw_data: raw_data.clone(),
                            field: "set_id/number".to_owned(),
                            reason: format!("DB lookup failed: {e}"),
                        });
                        skipped += 1;
                        continue;
                    }
                }
            }
        } else {
            None
        };

        // Condition
        let cond_str = get(&condition_col);
        let condition = match condition_from_str(cond_str) {
            Some(c) => c,
            None => {
                errors.push(ErrorRow {
                    row_number,
                    raw_data: raw_data.clone(),
                    field: "condition".to_owned(),
                    reason: format!("Unrecognised condition '{cond_str}'"),
                });
                skipped += 1;
                continue;
            }
        };

        // Quantity
        let qty_str = get(&quantity_col);
        let quantity: i32 = match qty_str.parse::<i32>() {
            Ok(q) if q > 0 => q,
            Ok(_) => {
                errors.push(ErrorRow {
                    row_number,
                    raw_data: raw_data.clone(),
                    field: "quantity".to_owned(),
                    reason: "quantity must be > 0".to_owned(),
                });
                skipped += 1;
                continue;
            }
            Err(_) => {
                errors.push(ErrorRow {
                    row_number,
                    raw_data: raw_data.clone(),
                    field: "quantity".to_owned(),
                    reason: format!("'{qty_str}' is not a valid integer"),
                });
                skipped += 1;
                continue;
            }
        };

        // Optional fields
        let cost_cents: Option<i32> = if let Some(col) = &cost_col {
            let s = get(col);
            if s.is_empty() {
                None
            } else {
                match s.trim_start_matches('$').replace(',', "").parse::<f64>() {
                    Ok(v) => Some((v * 100.0).round() as i32),
                    Err(_) => {
                        errors.push(ErrorRow {
                            row_number,
                            raw_data: raw_data.clone(),
                            field: "acquisition_cost".to_owned(),
                            reason: format!("'{s}' is not a valid number"),
                        });
                        skipped += 1;
                        continue;
                    }
                }
            }
        } else {
            None
        };

        let grader: Option<&str> = grader_col
            .as_deref()
            .map(|col| get(col))
            .filter(|s| !s.is_empty())
            .and_then(grader_from_str);

        let cert_number: Option<String> = cert_col
            .as_deref()
            .map(|col| get(col))
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);

        let notes: Option<String> = notes_col
            .as_deref()
            .map(|col| get(col))
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);

        // Persist
        if let (Some(grader), Some(cert)) = (grader, &cert_number) {
            // Graded row → card_instance; dedup by (workspace_id, grader, cert_number)
            let exists: bool = sqlx::query(
                "SELECT EXISTS(
                    SELECT 1 FROM card_instances
                    WHERE workspace_id = $1 AND grader = $2 AND cert_number = $3
                )::bool",
            )
            .bind(workspace_id)
            .bind(grader)
            .bind(cert.as_str())
            .fetch_one(&state.pool)
            .await
            .and_then(|r| r.try_get::<bool, _>(0))
            .unwrap_or(false);

            if exists {
                errors.push(ErrorRow {
                    row_number,
                    raw_data: raw_data.clone(),
                    field: "cert_number".to_owned(),
                    reason: format!("Duplicate cert {grader} #{cert} already in catalogue"),
                });
                skipped += 1;
                continue;
            }

            sqlx::query(
                "INSERT INTO card_instances
                 (id, workspace_id, printing_id, grader, cert_number, grade,
                  verification_status, acquisition_cost_cents, notes)
                 VALUES ($1, $2, $3, $4, $5, $6, 'unverified', $7, $8)
                 ON CONFLICT (workspace_id, grader, cert_number) DO NOTHING",
            )
            .bind(Uuid::new_v4())
            .bind(workspace_id)
            .bind(printing_id)
            .bind(grader)
            .bind(cert.as_str())
            .bind(condition)
            .bind(cost_cents)
            .bind(notes.as_deref())
            .execute(&state.pool)
            .await?;
        } else {
            // Raw row → inventory_item; upsert increments quantity
            sqlx::query(
                "INSERT INTO inventory_items
                 (id, workspace_id, printing_id, condition, quantity,
                  acquisition_cost_cents, notes)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (workspace_id, printing_id, condition)
                 DO UPDATE SET
                     quantity = inventory_items.quantity + EXCLUDED.quantity,
                     updated_at = now()",
            )
            .bind(Uuid::new_v4())
            .bind(workspace_id)
            .bind(printing_id)
            .bind(condition)
            .bind(quantity)
            .bind(cost_cents)
            .bind(notes.as_deref())
            .execute(&state.pool)
            .await?;
        }

        imported += 1;

        if processed % 100 == 0 {
            sqlx::query(
                "UPDATE import_jobs
                 SET processed_rows=$1, imported_rows=$2, skipped_rows=$3
                 WHERE id=$4",
            )
            .bind(processed as i64)
            .bind(imported as i64)
            .bind(skipped as i64)
            .bind(job_id)
            .execute(&state.pool)
            .await?;
        }
    }

    let error_report_json: Option<serde_json::Value> = if errors.is_empty() {
        None
    } else {
        Some(serde_json::to_value(&errors)?)
    };

    sqlx::query(
        "UPDATE import_jobs
         SET status = 'done', processed_rows=$1, imported_rows=$2,
             skipped_rows=$3, error_report=$4, completed_at=now()
         WHERE id=$5",
    )
    .bind(processed as i64)
    .bind(imported as i64)
    .bind(skipped as i64)
    .bind(error_report_json)
    .bind(job_id)
    .execute(&state.pool)
    .await?;

    let _ = fs::remove_file(&path).await;

    tracing::info!(job_id = %job_id, imported, skipped, "CSV import complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    }
}

fn upload_file_path(base_dir: &str, job_id: Uuid) -> PathBuf {
    PathBuf::from(base_dir)
        .join("uploads")
        .join(format!("{job_id}.csv"))
}

fn parse_preview(
    csv_text: &str,
) -> Result<(Vec<String>, Vec<HashMap<String, String>>), csv::Error> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(csv_text.as_bytes());

    let headers: Vec<String> = rdr.headers()?.iter().map(ToOwned::to_owned).collect();
    let mut rows = Vec::new();

    for result in rdr.records().take(5) {
        let record = result?;
        let mut map = HashMap::new();
        for (i, h) in headers.iter().enumerate() {
            map.insert(h.clone(), record.get(i).unwrap_or("").to_owned());
        }
        rows.push(map);
    }

    Ok((headers, rows))
}

fn generate_import_template() -> String {
    concat!(
        "set_id,number,name,condition,quantity,acquisition_cost,grader,cert_number,notes\r\n",
        "base1,4,Charizard,near_mint,1,150.00,,,\r\n",
        "# Graded example:\r\n",
        "base1,4,Charizard,near_mint,1,5000.00,PSA,12345678,\r\n",
    )
    .to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_stripping() {
        let with_bom = b"\xEF\xBB\xBFhello";
        assert_eq!(strip_bom(with_bom), b"hello");
        assert_eq!(strip_bom(b"hello"), b"hello");
    }

    #[test]
    fn auto_detect_mapping() {
        let headers: Vec<String> = ["Set Code", "Number", "Condition", "Qty", "Cost"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let m = ColumnMap::auto_detect(&headers);
        assert!(m.set_id.is_some());
        assert!(m.number.is_some());
        assert!(m.condition.is_some());
        assert!(m.quantity.is_some());
        assert!(m.acquisition_cost.is_some());
    }

    #[test]
    fn column_map_validate_ok() {
        let m = ColumnMap {
            condition: Some("Condition".to_owned()),
            quantity: Some("Qty".to_owned()),
            ..Default::default()
        };
        assert!(m.validate().is_ok());
    }

    #[test]
    fn column_map_validate_missing_condition() {
        let m = ColumnMap {
            quantity: Some("Qty".to_owned()),
            ..Default::default()
        };
        assert!(m.validate().is_err());
    }

    #[test]
    fn condition_flexible_parse() {
        assert_eq!(condition_from_str("NM"), Some("near_mint"));
        assert_eq!(condition_from_str("near mint"), Some("near_mint"));
        assert_eq!(condition_from_str("LP"), Some("lightly_played"));
        assert_eq!(condition_from_str("garbage"), None);
    }

    #[test]
    fn preview_parse() {
        let csv = "set_id,number,condition\nbase1,4,NM\nbase1,5,LP\n";
        let (headers, rows) = parse_preview(csv).unwrap();
        assert_eq!(headers, vec!["set_id", "number", "condition"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["set_id"], "base1");
    }

    #[test]
    fn template_starts_with_header() {
        let t = generate_import_template();
        assert!(t.starts_with("set_id,number,name"));
    }
}
