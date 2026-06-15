use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::workspace::MemberRole;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceInvite {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: MemberRole,
    pub token: String,
    pub invited_by: Uuid,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub resent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl WorkspaceInvite {
    pub fn is_pending(&self) -> bool {
        self.accepted_at.is_none()
            && self.cancelled_at.is_none()
            && self.expires_at > Utc::now()
    }
}
