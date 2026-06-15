use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{
        claims::{encode_access_token, Claims, ACCESS_TOKEN_TTL_SECS},
        service::jwt_secret,
    },
    error::AppError,
    models::workspace::{MemberRole, WorkspaceKind},
};

use super::authz::AuthContext;

#[derive(sqlx::FromRow)]
struct WsListRow {
    id: Uuid,
    name: String,
    kind: WorkspaceKind,
    created_at: DateTime<Utc>,
    role: MemberRole,
}

#[derive(sqlx::FromRow)]
struct WsRoleRow {
    kind: WorkspaceKind,
    role: MemberRole,
}

#[derive(sqlx::FromRow)]
struct MemberListRow {
    user_id: Uuid,
    email: String,
    name: String,
    role: MemberRole,
    joined_at: DateTime<Utc>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route(
            "/workspaces/:id",
            get(get_workspace),
        )
        .route("/workspaces/:id/activate", post(activate_workspace))
        .route("/workspaces/:id/members", get(list_members))
        .route("/workspaces/:id/members/:uid", delete(remove_member))
}

// ─── DTOs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub kind: WorkspaceKind,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceDto {
    pub id: Uuid,
    pub name: String,
    pub kind: WorkspaceKind,
    pub role: MemberRole,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MemberDto {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: MemberRole,
    pub joined_at: chrono::DateTime<Utc>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

async fn list_workspaces(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<WsListRow> = sqlx::query_as(
        r#"SELECT w.id, w.name, w.kind, w.created_at, wm.role
           FROM workspaces w
           JOIN workspace_members wm ON wm.workspace_id = w.id
           WHERE wm.user_id = $1
           ORDER BY w.created_at ASC"#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await?;

    let dtos: Vec<WorkspaceDto> = rows
        .into_iter()
        .map(|r| WorkspaceDto {
            id: r.id,
            name: r.name,
            kind: r.kind,
            role: r.role,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(dtos))
}

async fn create_workspace(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = body.name.trim().to_owned();
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest(
            "workspace name must be 1–100 characters".into(),
        ));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();
    let kind_str = match body.kind {
        WorkspaceKind::Seller => "seller",
        WorkspaceKind::Collector => "collector",
    };

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO workspaces (id, name, kind, created_at) VALUES ($1,$2,$3::workspace_kind,$4)",
    )
    .bind(id)
    .bind(&name)
    .bind(kind_str)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, joined_at) VALUES ($1,$2,'owner',$3)",
    )
    .bind(id)
    .bind(auth.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(WorkspaceDto {
            id,
            name,
            kind: body.kind,
            role: MemberRole::Owner,
            created_at: now,
        }),
    ))
}

async fn get_workspace(
    auth: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let row: WsListRow = sqlx::query_as(
        r#"SELECT w.id, w.name, w.kind, w.created_at, wm.role
           FROM workspaces w
           JOIN workspace_members wm ON wm.workspace_id = w.id
           WHERE w.id = $1 AND wm.user_id = $2"#,
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(WorkspaceDto {
        id: row.id,
        name: row.name,
        kind: row.kind,
        role: row.role,
        created_at: row.created_at,
    }))
}

/// Re-issue a JWT for a different workspace the caller is a member of.
async fn activate_workspace(
    auth: AuthContext,
    State(state): State<AppState>,
    Path(target_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let row: WsRoleRow = sqlx::query_as(
        r#"SELECT w.kind, wm.role
           FROM workspaces w
           JOIN workspace_members wm ON wm.workspace_id = w.id
           WHERE w.id = $1 AND wm.user_id = $2"#,
    )
    .bind(target_id)
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Forbidden("not a member of that workspace".into()))?;

    let now = Utc::now();
    let claims = Claims {
        sub: auth.user_id,
        email: auth.email,
        workspace_id: target_id,
        workspace_kind: row.kind,
        role: row.role,
        iat: now.timestamp(),
        exp: now.timestamp() + ACCESS_TOKEN_TTL_SECS,
    };
    let access_token = encode_access_token(&claims, jwt_secret().as_bytes())?;
    Ok(Json(serde_json::json!({ "access_token": access_token })))
}

async fn list_members(
    auth: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if auth.workspace_id != id {
        return Err(AppError::Forbidden("not the active workspace".into()));
    }
    let rows: Vec<MemberListRow> = sqlx::query_as(
        r#"SELECT u.id AS user_id, u.email, u.name, wm.role, wm.joined_at
           FROM workspace_members wm
           JOIN users u ON u.id = wm.user_id
           WHERE wm.workspace_id = $1
           ORDER BY wm.joined_at ASC"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    let dtos: Vec<MemberDto> = rows
        .into_iter()
        .map(|r| MemberDto {
            user_id: r.user_id,
            email: r.email,
            name: r.name,
            role: r.role,
            joined_at: r.joined_at,
        })
        .collect();

    Ok(Json(dtos))
}

async fn remove_member(
    auth: AuthContext,
    State(state): State<AppState>,
    Path((workspace_id, target_uid)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    if auth.workspace_id != workspace_id {
        return Err(AppError::Forbidden("not the active workspace".into()));
    }
    auth.require_owner()?;

    // Enforce last-owner invariant
    let count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = $1 AND role = 'owner'",
    )
    .bind(workspace_id)
    .fetch_one(&state.pool)
    .await?;
    let is_only_owner = count.unwrap_or(0) <= 1;

    let target_role: Option<String> = sqlx::query_scalar(
        "SELECT role::text FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(workspace_id)
    .bind(target_uid)
    .fetch_optional(&state.pool)
    .await?;

    if target_role.as_deref() == Some("owner") && is_only_owner {
        return Err(AppError::BadRequest("cannot remove the only owner".into()));
    }

    sqlx::query("DELETE FROM workspace_members WHERE user_id = $1 AND workspace_id = $2")
        .bind(target_uid)
        .bind(workspace_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
