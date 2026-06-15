pub mod error;
pub mod models;
pub mod pos;

pub use error::AppError;
pub use pos::mapping::{MappingService, PosProductMapping, UnmappedPosSku};
pub use pos::reconcile::{
    NullPosProvider, PosInventoryItem, PosSale, PosSaleLine, PosProvider,
    ReconciliationDiscrepancy, ReconciliationReport, ReconciliationService, Transaction,
    TransactionLine,
};

use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub workspace_id: uuid::Uuid,
    pub pos_provider: Arc<dyn PosProvider>,
}
