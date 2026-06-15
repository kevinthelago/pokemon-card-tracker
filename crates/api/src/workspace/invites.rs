//! T1 — Invites & membership management API
//!
//! Seller-only. All mutating endpoints require the caller to be an `owner`
//! of the target workspace. The last-owner invariant is enforced before any
//! role change or removal.
//!
//! Uses runtime sqlx queries (no `query!` macro) so the workspace compiles
//! without a DATABASE_URL at check time.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use lettre::{message::header::ContentType, AsyncTransport, Message as EmailMessage};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::{AuthUser, OptionalAuthUser},
    error::AppError,
    models::workspace::{MemberRole, WorkspaceKind},
};

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces/:wid/team", get(list_team))
        .route("/workspaces/:wid/invites", post(send_invite))
        .route("/workspaces/:wid/invites/:iid/resend", post(resend_invite))
        .route("/workspaces/:wid/invites/:iid", delete(cancel_invite))
        .route("/workspaces/:wid/members/:uid", patch(change_member_role))
        .route("/workspaces/:wid/members/:uid", delete(remove_member))
        .route("/invites/accept/:token", get(accept_invite))
}

// ─── Request / Response types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendInviteBody {
    pub email: String,
    pub role: MemberRole,
}

#[derive(Debug, Deserialize)]
pub struct ChangeRoleBody {
    pub role: MemberRole,
}

#[derive(Debug, Serialize)]
pub struct MemberDto {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct InviteDto {
    pub id: Uuid,
    pub email: String,
    pub role: MemberRole,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub resent_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct TeamResponse {
    pub members: Vec<MemberDto>,
    pub pending_invites: Vec<InviteDto>,
}

// ─── Internal query row types ─────────────────────────────────────────────────

#[derive(FromRow)]
struct WorkspaceAccessRow {
    kind: WorkspaceKind,
    role: Option<MemberRole>,
}

#[derive(FromRow)]
struct MemberRow {
    user_id: Uuid,
    role: MemberRole,
    joined_at: DateTime<Utc>,
    email: String,
    name: String,
}

#[derive(FromRow)]
struct InviteRow {
    id: Uuid,
    email: String,
    role: MemberRole,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    resent_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct InviteRowWithToken {
    id: Uuid,
    email: String,
    role: MemberRole,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    resent_at: Option<DateTime<Utc>>,
    token: String,
}

#[derive(FromRow)]
struct AcceptRow {
    id: Uuid,
    workspace_id: Uuid,
    email: String,
    role: MemberRole,
    expires_at: DateTime<Utc>,
    accepted_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
}

// ─── Guards ──────────────────────────────────────────────────────────────────

async fn require_owner(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    caller_id: Uuid,
) -> Result<(), AppError> {
    let row = sqlx::query_as::<_, WorkspaceAccessRow>(
        r#"
        SELECT w.kind, wm.role
        FROM workspaces w
        LEFT JOIN workspace_members wm
               ON wm.workspace_id = w.id AND wm.user_id = $2
        WHERE w.id = $1
        "#,
    )
    .bind(workspace_id)
    .bind(caller_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    if !matches!(row.kind, WorkspaceKind::Seller) {
        return Err(AppError::Forbidden(
            "team management is only available for seller workspaces".into(),
        ));
    }
    match row.role {
        Some(MemberRole::Owner) => Ok(()),
        _ => Err(AppError::Forbidden(
            "only owners can manage team members".into(),
        )),
    }
}

async fn count_owners(pool: &sqlx::PgPool, workspace_id: Uuid) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_members WHERE workspace_id = $1 AND role = 'owner'",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// GET /api/workspaces/:wid/team
async fn list_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
) -> Result<Json<TeamResponse>, AppError> {
    require_owner(&state.pool, wid, auth.id).await?;

    let members = sqlx::query_as::<_, MemberRow>(
        r#"
        SELECT wm.user_id, wm.role, wm.joined_at, u.email, u.name
        FROM workspace_members wm
        JOIN users u ON u.id = wm.user_id
        WHERE wm.workspace_id = $1
        ORDER BY wm.joined_at ASC
        "#,
    )
    .bind(wid)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|r| MemberDto {
        user_id: r.user_id,
        email: r.email,
        name: r.name,
        role: r.role,
        joined_at: r.joined_at,
    })
    .collect();

    let pending_invites = sqlx::query_as::<_, InviteRow>(
        r#"
        SELECT id, email, role, expires_at, created_at, resent_at
        FROM workspace_invites
        WHERE workspace_id = $1
          AND accepted_at IS NULL
          AND cancelled_at IS NULL
          AND expires_at > NOW()
        ORDER BY created_at ASC
        "#,
    )
    .bind(wid)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|r| InviteDto {
        id: r.id,
        email: r.email,
        role: r.role,
        expires_at: r.expires_at,
        created_at: r.created_at,
        resent_at: r.resent_at,
    })
    .collect();

    Ok(Json(TeamResponse {
        members,
        pending_invites,
    }))
}

/// POST /api/workspaces/:wid/invites
async fn send_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(wid): Path<Uuid>,
    Json(body): Json<SendInviteBody>,
) -> Result<(StatusCode, Json<InviteDto>), AppError> {
    require_owner(&state.pool, wid, auth.id).await?;

    let email = body.email.trim().to_lowercase();

    let is_member: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM workspace_members wm
            JOIN users u ON u.id = wm.user_id
            WHERE wm.workspace_id = $1 AND u.email = $2
        )
        "#,
    )
    .bind(wid)
    .bind(&email)
    .fetch_one(&state.pool)
    .await?;

    if is_member {
        return Err(AppError::Conflict(
            "this email address is already a member of the workspace".into(),
        ));
    }

    let duplicate: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM workspace_invites
            WHERE workspace_id = $1
              AND email = $2
              AND accepted_at IS NULL
              AND cancelled_at IS NULL
              AND expires_at > NOW()
        )
        "#,
    )
    .bind(wid)
    .bind(&email)
    .fetch_one(&state.pool)
    .await?;

    if duplicate {
        return Err(AppError::Conflict(
            "a pending invite already exists for this email address".into(),
        ));
    }

    let token = generate_token();
    let token_for_email = token.clone();

    let invite = sqlx::query_as::<_, InviteRow>(
        r#"
        INSERT INTO workspace_invites (workspace_id, email, role, token, invited_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, email, role, expires_at, created_at, resent_at
        "#,
    )
    .bind(wid)
    .bind(&email)
    .bind(body.role)
    .bind(&token)
    .bind(auth.id)
    .fetch_one(&state.pool)
    .await?;

    // Fire-and-forget; failure leaves the invite pending/resendable.
    let mailer = state.mailer.clone();
    let base_url = state.base_url.clone();
    let invite_email = email.clone();
    tokio::spawn(async move {
        if let Err(e) = send_invite_email(&mailer, &base_url, &invite_email, &token_for_email).await
        {
            tracing::warn!("invite email failed for {invite_email}: {e}");
        }
    });

    Ok((
        StatusCode::CREATED,
        Json(InviteDto {
            id: invite.id,
            email: invite.email,
            role: invite.role,
            expires_at: invite.expires_at,
            created_at: invite.created_at,
            resent_at: invite.resent_at,
        }),
    ))
}

/// POST /api/workspaces/:wid/invites/:iid/resend
async fn resend_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, iid)): Path<(Uuid, Uuid)>,
) -> Result<Json<InviteDto>, AppError> {
    require_owner(&state.pool, wid, auth.id).await?;

    let invite = sqlx::query_as::<_, InviteRowWithToken>(
        r#"
        UPDATE workspace_invites
           SET resent_at  = NOW(),
               expires_at = NOW() + INTERVAL '7 days'
         WHERE id = $1
           AND workspace_id = $2
           AND accepted_at IS NULL
           AND cancelled_at IS NULL
        RETURNING id, email, role, expires_at, created_at, resent_at, token
        "#,
    )
    .bind(iid)
    .bind(wid)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let mailer = state.mailer.clone();
    let base_url = state.base_url.clone();
    let token = invite.token.clone();
    let email = invite.email.clone();
    tokio::spawn(async move {
        if let Err(e) = send_invite_email(&mailer, &base_url, &email, &token).await {
            tracing::warn!("resend email failed for {email}: {e}");
        }
    });

    Ok(Json(InviteDto {
        id: invite.id,
        email: invite.email,
        role: invite.role,
        expires_at: invite.expires_at,
        created_at: invite.created_at,
        resent_at: invite.resent_at,
    }))
}

/// DELETE /api/workspaces/:wid/invites/:iid
async fn cancel_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, iid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_owner(&state.pool, wid, auth.id).await?;

    let updated = sqlx::query(
        r#"
        UPDATE workspace_invites
           SET cancelled_at = NOW()
         WHERE id = $1
           AND workspace_id = $2
           AND accepted_at IS NULL
           AND cancelled_at IS NULL
        "#,
    )
    .bind(iid)
    .bind(wid)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/workspaces/:wid/members/:uid
async fn change_member_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, uid)): Path<(Uuid, Uuid)>,
    Json(body): Json<ChangeRoleBody>,
) -> Result<StatusCode, AppError> {
    require_owner(&state.pool, wid, auth.id).await?;

    if matches!(body.role, MemberRole::Staff) {
        let current_role: Option<MemberRole> = sqlx::query_scalar(
            "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(wid)
        .bind(uid)
        .fetch_optional(&state.pool)
        .await?;

        if matches!(current_role, Some(MemberRole::Owner))
            && count_owners(&state.pool, wid).await? <= 1
        {
            return Err(AppError::Forbidden(
                "cannot downgrade the last owner; promote another member to owner first".into(),
            ));
        }
    }

    let updated = sqlx::query(
        "UPDATE workspace_members SET role = $3 WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(wid)
    .bind(uid)
    .bind(body.role)
    .execute(&state.pool)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/workspaces/:wid/members/:uid
async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((wid, uid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_owner(&state.pool, wid, auth.id).await?;

    let current_role: Option<MemberRole> = sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
    )
    .bind(wid)
    .bind(uid)
    .fetch_optional(&state.pool)
    .await?;

    if matches!(current_role, Some(MemberRole::Owner)) && count_owners(&state.pool, wid).await? <= 1
    {
        return Err(AppError::Forbidden(
            "cannot remove the last owner of a workspace".into(),
        ));
    }

    let deleted =
        sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2")
            .bind(wid)
            .bind(uid)
            .execute(&state.pool)
            .await?
            .rows_affected();

    if deleted == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/invites/accept/:token
///
/// Authenticated → join workspace + mark accepted + redirect.
/// Unauthenticated → redirect to sign-up with token in query string.
async fn accept_invite(
    State(state): State<AppState>,
    OptionalAuthUser(maybe_user): OptionalAuthUser,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let invite = sqlx::query_as::<_, AcceptRow>(
        r#"
        SELECT id, workspace_id, email, role, expires_at, accepted_at, cancelled_at
        FROM workspace_invites
        WHERE token = $1
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    if invite.cancelled_at.is_some() {
        return Err(AppError::BadRequest(
            "this invitation has been cancelled".into(),
        ));
    }
    if invite.accepted_at.is_some() {
        return Err(AppError::BadRequest(
            "this invitation has already been used".into(),
        ));
    }
    if invite.expires_at < Utc::now() {
        return Err(AppError::BadRequest(
            "this invitation has expired; ask the workspace owner to resend it".into(),
        ));
    }

    let user = match maybe_user {
        Some(u) => u,
        None => {
            let redirect_url = format!("{}/signup?invite_token={}", state.base_url, token);
            return Ok(Redirect::temporary(&redirect_url).into_response());
        }
    };

    if user.email.to_lowercase() != invite.email.to_lowercase() {
        return Err(AppError::Forbidden(
            "this invitation was sent to a different email address".into(),
        ));
    }

    let mut tx = state.pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO workspace_members (workspace_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (workspace_id, user_id) DO NOTHING
        "#,
    )
    .bind(invite.workspace_id)
    .bind(user.id)
    .bind(invite.role)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE workspace_invites SET accepted_at = NOW() WHERE id = $1")
        .bind(invite.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let redirect_url = format!("{}/workspaces/{}", state.base_url, invite.workspace_id);
    Ok(Redirect::temporary(&redirect_url).into_response())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect()
}

async fn send_invite_email(
    mailer: &lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    base_url: &str,
    to_email: &str,
    token: &str,
) -> anyhow::Result<()> {
    let accept_url = format!("{}/api/invites/accept/{}", base_url, token);
    let body = format!(
        "You have been invited to join a CardGuard workspace.\n\nAccept your invitation:\n{}\n\nThis link expires in 7 days.",
        accept_url
    );

    let email = EmailMessage::builder()
        .from("CardGuard <noreply@cardguard.app>".parse()?)
        .to(to_email.parse()?)
        .subject("You're invited to join a CardGuard workspace")
        .header(ContentType::TEXT_PLAIN)
        .body(body)?;

    mailer.send(email).await?;
    Ok(())
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_token_is_40_alphanumeric_chars() {
        let token = generate_token();
        assert_eq!(token.len(), 40);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn generate_token_is_unique() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn invite_dto_role_round_trips_through_serde() {
        use crate::models::workspace::MemberRole;
        let body = SendInviteBody {
            email: "a@b.com".into(),
            role: MemberRole::Owner,
        };
        let json = serde_json::to_string(&serde_json::json!({
            "email": body.email,
            "role": "owner"
        }))
        .unwrap();
        let parsed: SendInviteBody = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.email, "a@b.com");
        assert!(matches!(parsed.role, MemberRole::Owner));
    }

    #[test]
    fn change_role_body_deserializes_staff() {
        let json = r#"{"role":"staff"}"#;
        let body: ChangeRoleBody = serde_json::from_str(json).unwrap();
        assert!(matches!(
            body.role,
            crate::models::workspace::MemberRole::Staff
        ));
    }
}
