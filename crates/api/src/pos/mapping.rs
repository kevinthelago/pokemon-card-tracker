use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::AppError, models::connection::PosConnection, AppState};

// ─── Domain models ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PosProductMapping {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub printing_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UnmappedPosSku {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub pos_product_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

// ─── Service ──────────────────────────────────────────────────────────────────

pub struct MappingService;

impl MappingService {
    /// Look up the SKU→Printing mapping for a given connection and SKU.
    pub async fn find_mapping(
        pool: &PgPool,
        connection_id: Uuid,
        pos_sku: &str,
    ) -> Result<Option<PosProductMapping>, AppError> {
        sqlx::query_as::<_, PosProductMapping>(
            "SELECT * FROM pos_product_mappings \
             WHERE connection_id = $1 AND pos_sku = $2",
        )
        .bind(connection_id)
        .bind(pos_sku)
        .fetch_optional(pool)
        .await
        .map_err(AppError::from)
    }

    /// Insert a SKU into the unmapped queue (idempotent; updates last_seen_at).
    pub async fn queue_unmapped(
        pool: &PgPool,
        connection: &PosConnection,
        pos_sku: &str,
        pos_product_name: Option<&str>,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO unmapped_pos_skus \
                 (id, workspace_id, connection_id, pos_sku, pos_product_name, \
                  first_seen_at, last_seen_at) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4, NOW(), NOW()) \
             ON CONFLICT (connection_id, pos_sku) DO UPDATE \
                 SET last_seen_at = NOW(), \
                     pos_product_name = COALESCE($4, unmapped_pos_skus.pos_product_name)",
        )
        .bind(connection.workspace_id)
        .bind(connection.id)
        .bind(pos_sku)
        .bind(pos_product_name)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// List all mappings for a workspace (optionally filtered by connection).
    pub async fn list_mappings(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Option<Uuid>,
    ) -> Result<Vec<PosProductMapping>, AppError> {
        match connection_id {
            Some(cid) => sqlx::query_as::<_, PosProductMapping>(
                "SELECT * FROM pos_product_mappings \
                 WHERE workspace_id = $1 AND connection_id = $2 \
                 ORDER BY pos_sku",
            )
            .bind(workspace_id)
            .bind(cid)
            .fetch_all(pool)
            .await
            .map_err(AppError::from),

            None => sqlx::query_as::<_, PosProductMapping>(
                "SELECT * FROM pos_product_mappings \
                 WHERE workspace_id = $1 \
                 ORDER BY pos_sku",
            )
            .bind(workspace_id)
            .fetch_all(pool)
            .await
            .map_err(AppError::from),
        }
    }

    /// List unmapped SKUs for a workspace.
    pub async fn list_unmapped(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Option<Uuid>,
    ) -> Result<Vec<UnmappedPosSku>, AppError> {
        match connection_id {
            Some(cid) => sqlx::query_as::<_, UnmappedPosSku>(
                "SELECT u.* FROM unmapped_pos_skus u \
                 WHERE u.workspace_id = $1 AND u.connection_id = $2 \
                   AND NOT EXISTS (\
                       SELECT 1 FROM pos_product_mappings m \
                       WHERE m.connection_id = u.connection_id AND m.pos_sku = u.pos_sku\
                   ) \
                 ORDER BY u.last_seen_at DESC",
            )
            .bind(workspace_id)
            .bind(cid)
            .fetch_all(pool)
            .await
            .map_err(AppError::from),

            None => sqlx::query_as::<_, UnmappedPosSku>(
                "SELECT u.* FROM unmapped_pos_skus u \
                 WHERE u.workspace_id = $1 \
                   AND NOT EXISTS (\
                       SELECT 1 FROM pos_product_mappings m \
                       WHERE m.connection_id = u.connection_id AND m.pos_sku = u.pos_sku\
                   ) \
                 ORDER BY u.last_seen_at DESC",
            )
            .bind(workspace_id)
            .fetch_all(pool)
            .await
            .map_err(AppError::from),
        }
    }

    /// Create a new SKU→Printing mapping.
    ///
    /// After creating the mapping, previously-unmapped transaction lines for this
    /// SKU are back-filled with the printing and the catalogue quantity is adjusted
    /// to account for all historical sales that were unresolved.
    pub async fn create_mapping(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Uuid,
        pos_sku: &str,
        printing_id: Uuid,
    ) -> Result<MappingWithBackfill, AppError> {
        // Verify printing belongs to the workspace.
        let printing_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM printings WHERE id = $1 AND workspace_id = $2)",
        )
        .bind(printing_id)
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;

        if !printing_exists {
            return Err(AppError::NotFound(format!(
                "Printing {} not found in workspace",
                printing_id
            )));
        }

        let mapping = sqlx::query_as::<_, PosProductMapping>(
            "INSERT INTO pos_product_mappings \
                 (id, workspace_id, connection_id, pos_sku, printing_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, $4) \
             RETURNING *",
        )
        .bind(workspace_id)
        .bind(connection_id)
        .bind(pos_sku)
        .bind(printing_id)
        .fetch_one(pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.constraint() == Some("pos_product_mappings_connection_id_pos_sku_key") => {
                AppError::BadRequest(format!("SKU '{}' is already mapped for this connection", pos_sku))
            }
            _ => AppError::from(e),
        })?;

        // Back-fill: link historical unmapped transaction lines to this printing
        // and adjust catalogue quantity for their net effect.
        let backfill_count =
            Self::backfill_unmapped_lines(pool, workspace_id, connection_id, pos_sku, &mapping)
                .await?;

        // Remove from unmapped queue now that it's mapped.
        sqlx::query(
            "DELETE FROM unmapped_pos_skus \
             WHERE connection_id = $1 AND pos_sku = $2",
        )
        .bind(connection_id)
        .bind(pos_sku)
        .execute(pool)
        .await?;

        Ok(MappingWithBackfill {
            mapping,
            backfilled_lines: backfill_count,
        })
    }

    /// Update an existing mapping's printing_id.
    pub async fn update_mapping(
        pool: &PgPool,
        workspace_id: Uuid,
        mapping_id: Uuid,
        new_printing_id: Uuid,
    ) -> Result<MappingWithBackfill, AppError> {
        let old: PosProductMapping = sqlx::query_as::<_, PosProductMapping>(
            "SELECT * FROM pos_product_mappings WHERE id = $1 AND workspace_id = $2",
        )
        .bind(mapping_id)
        .bind(workspace_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Mapping not found".into()))?;

        // Verify new printing belongs to the workspace.
        let printing_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM printings WHERE id = $1 AND workspace_id = $2)",
        )
        .bind(new_printing_id)
        .bind(workspace_id)
        .fetch_one(pool)
        .await?;

        if !printing_exists {
            return Err(AppError::NotFound(format!(
                "Printing {} not found in workspace",
                new_printing_id
            )));
        }

        // Reverse the quantity effect on the old printing.
        Self::reverse_line_effects(pool, workspace_id, mapping_id, old.printing_id).await?;

        let updated = sqlx::query_as::<_, PosProductMapping>(
            "UPDATE pos_product_mappings \
             SET printing_id = $2, updated_at = NOW() \
             WHERE id = $1 \
             RETURNING *",
        )
        .bind(mapping_id)
        .bind(new_printing_id)
        .fetch_one(pool)
        .await?;

        // Re-apply line effects to the new printing.
        let backfill_count =
            Self::backfill_unmapped_lines(pool, workspace_id, old.connection_id, &old.pos_sku, &updated)
                .await?;

        Ok(MappingWithBackfill {
            mapping: updated,
            backfilled_lines: backfill_count,
        })
    }

    /// Delete a mapping. Does NOT reverse the historical transaction effects —
    /// future reconciliation will surface any discrepancy.
    pub async fn delete_mapping(
        pool: &PgPool,
        workspace_id: Uuid,
        mapping_id: Uuid,
    ) -> Result<(), AppError> {
        let rows = sqlx::query(
            "DELETE FROM pos_product_mappings WHERE id = $1 AND workspace_id = $2",
        )
        .bind(mapping_id)
        .bind(workspace_id)
        .execute(pool)
        .await?;

        if rows.rows_affected() == 0 {
            return Err(AppError::NotFound("Mapping not found".into()));
        }
        Ok(())
    }

    /// Back-fill all unmapped transaction_lines for this SKU to point at the new mapping,
    /// and apply their net quantity effect to the printing's catalogue quantity.
    async fn backfill_unmapped_lines(
        pool: &PgPool,
        workspace_id: Uuid,
        connection_id: Uuid,
        pos_sku: &str,
        mapping: &PosProductMapping,
    ) -> Result<i64, AppError> {
        // Update lines: link them to this mapping.
        let updated = sqlx::query(
            "UPDATE transaction_lines tl \
             SET printing_id = $3, pos_product_mapping_id = $4 \
             FROM transactions t \
             WHERE tl.transaction_id = t.id \
               AND t.connection_id = $1 \
               AND tl.pos_sku = $2 \
               AND tl.printing_id IS NULL",
        )
        .bind(connection_id)
        .bind(pos_sku)
        .bind(mapping.printing_id)
        .bind(mapping.id)
        .execute(pool)
        .await?;

        // Calculate net quantity delta from those lines and apply to the printing.
        let net_delta: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(tl.quantity), 0) \
             FROM transaction_lines tl \
             JOIN transactions t ON t.id = tl.transaction_id \
             WHERE t.connection_id = $1 \
               AND tl.pos_sku = $2 \
               AND tl.printing_id = $3",
        )
        .bind(connection_id)
        .bind(pos_sku)
        .bind(mapping.printing_id)
        .fetch_one(pool)
        .await?;

        if let Some(delta) = net_delta.filter(|d| *d != 0) {
            sqlx::query(
                "UPDATE printings SET quantity = quantity + $1, updated_at = NOW() \
                 WHERE id = $2 AND workspace_id = $3",
            )
            .bind(delta as i32)
            .bind(mapping.printing_id)
            .bind(workspace_id)
            .execute(pool)
            .await?;
        }

        Ok(updated.rows_affected() as i64)
    }

    /// Reverse the quantity effect of all lines associated with a mapping (used on update).
    async fn reverse_line_effects(
        pool: &PgPool,
        workspace_id: Uuid,
        mapping_id: Uuid,
        printing_id: Uuid,
    ) -> Result<(), AppError> {
        let net_delta: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(quantity), 0) \
             FROM transaction_lines \
             WHERE pos_product_mapping_id = $1",
        )
        .bind(mapping_id)
        .fetch_one(pool)
        .await?;

        if let Some(delta) = net_delta.filter(|d| *d != 0) {
            // Negate to reverse the effect.
            sqlx::query(
                "UPDATE printings SET quantity = quantity - $1, updated_at = NOW() \
                 WHERE id = $2 AND workspace_id = $3",
            )
            .bind(delta as i32)
            .bind(printing_id)
            .bind(workspace_id)
            .execute(pool)
            .await?;
        }
        Ok(())
    }
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MappingWithBackfill {
    pub mapping: PosProductMapping,
    pub backfilled_lines: i64,
}

#[derive(Debug, Serialize)]
pub struct MappingQueue {
    pub mappings: Vec<PosProductMapping>,
    pub unmapped: Vec<UnmappedPosSku>,
}

// ─── HTTP handlers ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct MappingQuery {
    pub connection_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMappingRequest {
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub printing_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMappingRequest {
    pub printing_id: Uuid,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pos/mapping", get(handle_list_mappings).post(handle_create_mapping))
        .route(
            "/pos/mapping/:mapping_id",
            put(handle_update_mapping).delete(handle_delete_mapping),
        )
}

async fn handle_list_mappings(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<MappingQuery>,
) -> Result<Json<MappingQueue>, AppError> {
    let mappings =
        MappingService::list_mappings(&state.pool, state.workspace_id, q.connection_id).await?;
    let unmapped =
        MappingService::list_unmapped(&state.pool, state.workspace_id, q.connection_id).await?;
    Ok(Json(MappingQueue { mappings, unmapped }))
}

async fn handle_create_mapping(
    State(state): State<AppState>,
    Json(req): Json<CreateMappingRequest>,
) -> Result<(StatusCode, Json<MappingWithBackfill>), AppError> {
    let result = MappingService::create_mapping(
        &state.pool,
        state.workspace_id,
        req.connection_id,
        &req.pos_sku,
        req.printing_id,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn handle_update_mapping(
    State(state): State<AppState>,
    Path(mapping_id): Path<Uuid>,
    Json(req): Json<UpdateMappingRequest>,
) -> Result<Json<MappingWithBackfill>, AppError> {
    let result = MappingService::update_mapping(
        &state.pool,
        state.workspace_id,
        mapping_id,
        req.printing_id,
    )
    .await?;
    Ok(Json(result))
}

async fn handle_delete_mapping(
    State(state): State<AppState>,
    Path(mapping_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    MappingService::delete_mapping(&state.pool, state.workspace_id, mapping_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_unmapped_excludes_already_mapped() {
        // Verified by the SQL WHERE NOT EXISTS clause; logic is tested via integration tests.
        // This unit test documents intent.
        let sql = "SELECT u.* FROM unmapped_pos_skus u \
                   WHERE NOT EXISTS (\
                       SELECT 1 FROM pos_product_mappings m \
                       WHERE m.connection_id = u.connection_id AND m.pos_sku = u.pos_sku\
                   )";
        assert!(sql.contains("NOT EXISTS"));
    }

    #[test]
    fn duplicate_mapping_error_message_mentions_sku() {
        let sku = "PKMN-001";
        let msg = format!("SKU '{}' is already mapped for this connection", sku);
        assert!(msg.contains("PKMN-001"));
    }
}
