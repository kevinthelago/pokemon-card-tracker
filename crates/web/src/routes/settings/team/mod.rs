//! T2 — Team settings UI (seller workspaces only)

mod invite_form;
mod member_list;
mod page;

pub use invite_form::InviteForm;
pub use member_list::MemberList;
pub use page::TeamPage;

// ─── Shared DTO types (mirror the API response shapes) ───────────────────────

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Staff,
}

impl MemberRole {
    pub fn label(&self) -> &'static str {
        match self {
            MemberRole::Owner => "Owner",
            MemberRole::Staff => "Staff",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberDto {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteDto {
    pub id: Uuid,
    pub email: String,
    pub role: MemberRole,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub resent_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamResponse {
    pub members: Vec<MemberDto>,
    pub pending_invites: Vec<InviteDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInviteBody {
    pub email: String,
    pub role: MemberRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRoleBody {
    pub role: MemberRole,
}
