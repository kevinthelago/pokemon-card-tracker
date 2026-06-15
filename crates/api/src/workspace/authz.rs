use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, HeaderMap},
};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{claims::decode_access_token, service::jwt_secret},
    error::AppError,
    models::workspace::{MemberRole, WorkspaceKind},
};

/// Full auth context for workspace-scoped endpoints.
///
/// Resolution:
/// 1. Decode JWT Bearer token → get user_id + default workspace_id.
/// 2. If `X-Workspace-Id` header present, use that workspace instead.
/// 3. Re-validate membership from DB (JWT claim is a hint only).
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub email: String,
    pub workspace_id: Uuid,
    pub workspace_kind: WorkspaceKind,
    pub role: MemberRole,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);

        let token = bearer_token(&parts.headers)?;
        let claims = decode_access_token(token, jwt_secret().as_bytes())?;

        let target_ws = header_workspace_id(&parts.headers).unwrap_or(claims.workspace_id);

        let row = sqlx::query!(
            r#"SELECT w.kind AS "kind: WorkspaceKind", wm.role AS "role: MemberRole"
               FROM workspace_members wm
               JOIN workspaces w ON w.id = wm.workspace_id
               WHERE wm.user_id = $1 AND wm.workspace_id = $2"#,
            claims.sub,
            target_ws,
        )
        .fetch_optional(&app.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::Forbidden("not a member of requested workspace".into()))?;

        Ok(AuthContext {
            user_id: claims.sub,
            email: claims.email,
            workspace_id: target_ws,
            workspace_kind: row.kind,
            role: row.role,
        })
    }
}

impl AuthContext {
    pub fn require_owner(&self) -> Result<(), AppError> {
        match self.role {
            MemberRole::Owner => Ok(()),
            MemberRole::Staff => Err(AppError::Forbidden("owner role required".into())),
        }
    }

    pub fn require_seller(&self) -> Result<(), AppError> {
        match self.workspace_kind {
            WorkspaceKind::Seller => Ok(()),
            WorkspaceKind::Collector => {
                Err(AppError::Forbidden("seller workspace required".into()))
            }
        }
    }

    pub fn require_seller_owner(&self) -> Result<(), AppError> {
        self.require_seller()?;
        self.require_owner()
    }
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)
}

fn header_workspace_id(headers: &HeaderMap) -> Option<Uuid> {
    headers
        .get("x-workspace-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
}
