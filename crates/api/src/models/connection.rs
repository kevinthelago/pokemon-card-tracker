use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stub model — owned by the connect-pos stream.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PosConnection {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub provider: String,
    pub display_name: String,
    pub sync_direction: String,
    pub is_active: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl PosConnection {
    pub fn pulls_from_pos(&self) -> bool {
        matches!(self.sync_direction.as_str(), "pull_only" | "bidirectional")
    }

    pub fn pushes_to_pos(&self) -> bool {
        matches!(self.sync_direction.as_str(), "push_only" | "bidirectional")
    }
}
