use crate::enums::{WorkspaceKind, WorkspaceRole};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    /// argon2id hash — never serialise in API responses.
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

/// A seller shop or a collector's personal collection.
/// Seller workspaces unlock POS sync, reconciliation, and fraud detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub kind: WorkspaceKind,
    pub owner_user_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Binds a user to a workspace with a role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub role: WorkspaceRole,
    pub created_at: DateTime<Utc>,
}

/// Email invitation to join a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub invited_by_user_id: Uuid,
    pub email: String,
    pub role: WorkspaceRole,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Invite {
    pub fn is_valid(&self) -> bool {
        self.accepted_at.is_none() && self.revoked_at.is_none() && self.expires_at > Utc::now()
    }
}
