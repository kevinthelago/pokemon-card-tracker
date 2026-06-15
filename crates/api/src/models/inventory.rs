use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stub model — owned by the manage-inventory stream.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Printing {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub card_name: String,
    pub set_code: String,
    pub set_number: String,
    pub quantity: i32,
}
