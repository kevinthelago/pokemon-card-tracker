use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    identity::{IdentityService, SearchQuery},
    items::{AddGradedItemRequest, AddRawItemRequest, ItemsService},
};
use crate::{app::AppState, error::AppError};
use domain::{BarcodeKind, ItemKind, VerificationStatus};

// ── Router ─────────────────────────────────────────────────────────────────

/// Mount this in app.rs:  `Router::new().nest("/api", catalogue::routes::router())`
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/printings/search", get(search_printings))
        .route("/catalogue/scan", post(scan_barcode))
        .route("/catalogue/items", post(add_item))
}

// ── GET /printings/search ──────────────────────────────────────────────────

async fn search_printings(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let svc = IdentityService::new(state.pool.clone(), state.tcg_client.clone());
    let results = svc.search(&query).await?;
    Ok(Json(results))
}

// ── POST /catalogue/scan ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ScanRequest {
    barcode: String,
    kind: BarcodeKind,
    /// Reserved for workspace-scoped scan audit logging (auth stream).
    #[serde(default)]
    workspace_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct ScanResponse {
    barcode: String,
    resolved_as: Option<String>,
    printing: Option<domain::Printing>,
    sealed_product: Option<domain::SealedProduct>,
    grading_verification: Option<domain::GradingVerification>,
    needs_manual_entry: bool,
    error: Option<String>,
}

async fn scan_barcode(
    State(state): State<AppState>,
    Json(body): Json<ScanRequest>,
) -> Result<impl IntoResponse, AppError> {
    let svc = IdentityService::new(state.pool.clone(), state.tcg_client.clone());

    let resolution = resolve_scan(&svc, &body).await;

    Ok((StatusCode::OK, Json(resolution)))
}

async fn resolve_scan(svc: &IdentityService, req: &ScanRequest) -> ScanResponse {
    match req.kind {
        BarcodeKind::Upc => resolve_upc_scan(svc, req).await,
        BarcodeKind::Cert => resolve_cert_scan(req).await,
        BarcodeKind::Auto => {
            // Auto: try UPC first (numeric barcodes), then cert format.
            if req.barcode.chars().all(|c| c.is_ascii_digit()) && req.barcode.len() >= 8 {
                resolve_upc_scan(svc, req).await
            } else {
                resolve_cert_scan(req).await
            }
        }
    }
}

async fn resolve_upc_scan(svc: &IdentityService, req: &ScanRequest) -> ScanResponse {
    match svc.resolve_upc(&req.barcode).await {
        Ok(Some(product)) => ScanResponse {
            barcode: req.barcode.clone(),
            resolved_as: Some("sealed_product".into()),
            printing: None,
            sealed_product: Some(product),
            grading_verification: None,
            needs_manual_entry: false,
            error: None,
        },
        Ok(None) => ScanResponse {
            barcode: req.barcode.clone(),
            resolved_as: None,
            printing: None,
            sealed_product: None,
            grading_verification: None,
            needs_manual_entry: true,
            error: Some("UPC not recognized — manual entry required".into()),
        },
        Err(e) => ScanResponse {
            barcode: req.barcode.clone(),
            resolved_as: None,
            printing: None,
            sealed_product: None,
            grading_verification: None,
            needs_manual_entry: true,
            error: Some(e.to_string()),
        },
    }
}

async fn resolve_cert_scan(req: &ScanRequest) -> ScanResponse {
    // Parse "GRADER-CERTNUMBER" format (e.g. "PSA-12345678").
    // In v1, grading verification is attempted by the grading adapter (future
    // stream); here we return an unverified placeholder so the UI can still
    // save the card flagged as unverified.
    let (grader, cert) = parse_cert_barcode(&req.barcode);

    ScanResponse {
        barcode: req.barcode.clone(),
        resolved_as: Some("graded".into()),
        printing: None,
        sealed_product: None,
        grading_verification: Some(domain::GradingVerification {
            grader,
            cert_number: cert,
            grade: None,
            card_name: None,
            set_name: None,
            year: None,
            // Grading adapter (verify-graded-card stream) will update this;
            // default to unverified so the card can still be saved.
            status: VerificationStatus::Unverified,
            raw_response: None,
        }),
        needs_manual_entry: false,
        error: None,
    }
}

/// Attempt to split a cert barcode into (grader, cert_number).
/// Falls back to ("UNKNOWN", raw_barcode) if format is unrecognised.
fn parse_cert_barcode(barcode: &str) -> (String, String) {
    // PSA cert barcodes are 8-digit numbers; BGS uses alphanumeric.
    // Graded slab barcodes sometimes include the grader prefix separated by '-'.
    if let Some((prefix, rest)) = barcode.split_once('-') {
        let prefix_upper = prefix.to_uppercase();
        if ["PSA", "BGS", "CGC", "SGC", "HGA"].contains(&prefix_upper.as_str()) {
            return (prefix_upper, rest.to_string());
        }
    }

    // Numeric-only → assume PSA.
    if barcode.chars().all(|c| c.is_ascii_digit()) {
        return ("PSA".into(), barcode.to_string());
    }

    ("UNKNOWN".into(), barcode.to_string())
}

// ── POST /catalogue/items ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AddItemRequest {
    workspace_id: Uuid,
    item_kind: ItemKind,
    // Raw / sealed fields
    printing_id: Option<Uuid>,
    /// Reserved for sealed-product items (manage-inventory stream).
    sealed_product_id: Option<Uuid>,
    condition: Option<String>,
    #[serde(default = "one")]
    quantity: i32,
    acquisition_cost_cents: Option<i32>,
    // Graded fields
    grader: Option<String>,
    cert_number: Option<String>,
    grade: Option<String>,
    #[serde(default = "unverified")]
    verification_status: VerificationStatus,
    // Common
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    photos: Vec<String>,
}

fn one() -> i32 {
    1
}
fn unverified() -> VerificationStatus {
    VerificationStatus::Unverified
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AddItemResponse {
    Inventory(domain::InventoryItem),
    Instance(domain::CardInstance),
}

async fn add_item(
    State(state): State<AppState>,
    Json(body): Json<AddItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    let svc = ItemsService::new(state.pool.clone());

    match body.item_kind {
        ItemKind::Raw | ItemKind::Sealed => {
            let printing_id = body.printing_id.ok_or_else(|| {
                AppError::Validation("printing_id is required for raw/sealed items".into())
            })?;
            let condition = body.condition.ok_or_else(|| {
                AppError::Validation("condition is required for raw items".into())
            })?;

            let req = AddRawItemRequest {
                workspace_id: body.workspace_id,
                printing_id,
                condition,
                quantity: body.quantity,
                acquisition_cost_cents: body.acquisition_cost_cents,
                notes: body.notes,
                photos: body.photos,
            };

            let item = svc.upsert_inventory_item(&req).await?;
            Ok((StatusCode::CREATED, Json(AddItemResponse::Inventory(item))))
        }
        ItemKind::Graded => {
            let grader = body.grader.ok_or_else(|| {
                AppError::Validation("grader is required for graded items".into())
            })?;
            let cert_number = body.cert_number.ok_or_else(|| {
                AppError::Validation("cert_number is required for graded items".into())
            })?;

            let req = AddGradedItemRequest {
                workspace_id: body.workspace_id,
                printing_id: body.printing_id,
                grader,
                cert_number,
                grade: body.grade,
                verification_status: body.verification_status,
                acquisition_cost_cents: body.acquisition_cost_cents,
                notes: body.notes,
                photos: body.photos,
            };

            let instance = svc.add_card_instance(&req).await?;
            Ok((
                StatusCode::CREATED,
                Json(AddItemResponse::Instance(instance)),
            ))
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_grader_prefix() {
        assert_eq!(
            parse_cert_barcode("PSA-12345678"),
            ("PSA".into(), "12345678".into())
        );
        assert_eq!(
            parse_cert_barcode("bgs-12345"),
            ("BGS".into(), "12345".into())
        );
        assert_eq!(
            parse_cert_barcode("CGC-ABC123"),
            ("CGC".into(), "ABC123".into())
        );
        assert_eq!(parse_cert_barcode("sgc-99"), ("SGC".into(), "99".into()));
        assert_eq!(parse_cert_barcode("HGA-55"), ("HGA".into(), "55".into()));
    }

    #[test]
    fn parse_numeric_only_assumes_psa() {
        let (grader, cert) = parse_cert_barcode("12345678");
        assert_eq!(grader, "PSA");
        assert_eq!(cert, "12345678");
    }

    #[test]
    fn parse_unknown_format_falls_back() {
        let (grader, cert) = parse_cert_barcode("SOMEUNKNOWNBARCODE");
        assert_eq!(grader, "UNKNOWN");
        assert_eq!(cert, "SOMEUNKNOWNBARCODE");
    }

    #[test]
    fn parse_unknown_prefix_falls_back() {
        let (grader, cert) = parse_cert_barcode("XYZ-12345");
        assert_eq!(grader, "UNKNOWN");
        assert_eq!(cert, "XYZ-12345");
    }
}
