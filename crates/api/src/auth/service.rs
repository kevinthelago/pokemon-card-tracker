use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    app::AppState,
    error::AppError,
    models::workspace::{MemberRole, WorkspaceKind},
};

use super::{
    claims::{encode_access_token, Claims, ACCESS_TOKEN_TTL_SECS},
    email::{send_password_reset_email, send_verification_email},
    models::{AuthResponse, DbUser},
    password::{hash_password, verify_password},
};

pub const REFRESH_TOKEN_TTL_DAYS: i64 = 30;
const EMAIL_VERIFY_TTL_HOURS: i64 = 24;
const PASSWORD_RESET_TTL_HOURS: i64 = 1;

// Local row structs — dynamic queries avoid compile-time DATABASE_URL requirement.
#[derive(sqlx::FromRow)]
struct WsRow {
    id: Uuid,
    kind: WorkspaceKind,
    role: MemberRole,
}

#[derive(sqlx::FromRow)]
struct RefreshTokenRow {
    id: Uuid,
    user_id: Uuid,
    family_id: Uuid,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    name: String,
}

#[derive(sqlx::FromRow)]
struct UserEmailRow {
    id: Uuid,
    email: String,
}

#[derive(sqlx::FromRow)]
struct TokenRow {
    id: Uuid,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
}

pub fn jwt_secret() -> String {
    std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-in-production-must-be-32-chars".into())
}

pub fn email_from() -> String {
    std::env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@cardguard.app".into())
}

fn token_hash(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

fn generate_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    hex::encode(bytes)
}

pub(crate) fn validate_password(pw: &str) -> Result<(), AppError> {
    if pw.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

fn validate_email(email: &str) -> Result<(), AppError> {
    if email.contains('@') && email.len() <= 254 {
        Ok(())
    } else {
        Err(AppError::BadRequest("invalid email address".into()))
    }
}

fn validate_name(name: &str) -> Result<(), AppError> {
    let t = name.trim();
    if t.is_empty() || t.len() > 100 {
        Err(AppError::BadRequest("name must be 1–100 characters".into()))
    } else {
        Ok(())
    }
}

// ─── Register ─────────────────────────────────────────────────────────────────

pub async fn register(
    state: &AppState,
    email: &str,
    name: &str,
    password: &str,
) -> Result<AuthResponse, AppError> {
    let email = email.to_lowercase();
    let email = email.trim();
    let name = name.trim();
    validate_email(email)?;
    validate_name(name)?;
    validate_password(password)?;

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
            .bind(email)
            .fetch_one(&state.pool)
            .await?;

    if exists {
        return Err(AppError::Conflict("email already registered".into()));
    }

    let password_hash = hash_password(password)?;
    let user_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let now = Utc::now();
    let workspace_name = format!("{name}'s Collection");

    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO users (id, email, name, password_hash, created_at) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(user_id)
    .bind(email)
    .bind(name)
    .bind(&password_hash)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO workspaces (id, name, kind, created_at) VALUES ($1,$2,'collector',$3)",
    )
    .bind(workspace_id)
    .bind(&workspace_name)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role, joined_at) VALUES ($1,$2,'owner',$3)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let raw_ev = generate_token();
    let ev_hash = token_hash(&raw_ev);
    let ev_expires = now + Duration::hours(EMAIL_VERIFY_TTL_HOURS);
    sqlx::query(
        "INSERT INTO email_verifications (id, user_id, token_hash, expires_at, created_at) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&ev_hash)
    .bind(ev_expires)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let verify_url = format!("{}/auth/verify-email?token={}", state.base_url, raw_ev);
    let _ = send_verification_email(&state.mailer, &email_from(), email, &verify_url).await;

    let (access_token, refresh_token) = issue_tokens(
        state,
        user_id,
        email,
        workspace_id,
        WorkspaceKind::Collector,
        MemberRole::Owner,
        None,
    )
    .await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
        user_id,
        email: email.to_owned(),
        name: name.to_owned(),
        workspace_id,
    })
}

// ─── Login ────────────────────────────────────────────────────────────────────

pub async fn login(
    state: &AppState,
    email: &str,
    password: &str,
) -> Result<AuthResponse, AppError> {
    let email = email.to_lowercase();
    let email = email.trim();

    let user: Option<DbUser> = sqlx::query_as(
        "SELECT id, email, name, password_hash, email_verified_at, created_at FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&state.pool)
    .await?;

    match &user {
        Some(u) => verify_password(password, &u.password_hash)?,
        None => {
            // constant-time dummy
            let _ = verify_password(password, "$argon2id$v=19$m=19456,t=2,p=1$ZHVtbXlzYWx0c2FsdA$dummyhashvalueherethisisnotreal0");
            return Err(AppError::Unauthorized);
        }
    }

    let user = user.unwrap();

    let row: WsRow = sqlx::query_as(
        r#"SELECT w.id, w.kind, wm.role
           FROM workspaces w
           JOIN workspace_members wm ON wm.workspace_id = w.id
           WHERE wm.user_id = $1
           ORDER BY CASE WHEN w.kind = 'collector' THEN 0 ELSE 1 END, w.created_at ASC
           LIMIT 1"#,
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    let (access_token, refresh_token) =
        issue_tokens(state, user.id, &user.email, row.id, row.kind, row.role, None).await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
        user_id: user.id,
        email: user.email.clone(),
        name: user.name,
        workspace_id: row.id,
    })
}

// ─── Refresh ──────────────────────────────────────────────────────────────────

pub async fn refresh(state: &AppState, raw_token: &str) -> Result<AuthResponse, AppError> {
    let h = token_hash(raw_token);
    let now = Utc::now();

    let row: RefreshTokenRow = sqlx::query_as(
        "SELECT id, user_id, family_id, expires_at, revoked_at FROM refresh_tokens WHERE token_hash = $1",
    )
    .bind(&h)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    if row.revoked_at.is_some() {
        // Reuse detected — revoke entire family
        sqlx::query(
            "UPDATE refresh_tokens SET revoked_at = $1 WHERE family_id = $2 AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(row.family_id)
        .execute(&state.pool)
        .await?;
        return Err(AppError::Unauthorized);
    }
    if row.expires_at < now {
        return Err(AppError::Unauthorized);
    }

    // Rotate: revoke old, issue new in same family
    sqlx::query("UPDATE refresh_tokens SET revoked_at = $1 WHERE id = $2")
        .bind(now)
        .bind(row.id)
        .execute(&state.pool)
        .await?;

    let user: UserRow = sqlx::query_as("SELECT id, email, name FROM users WHERE id = $1")
        .bind(row.user_id)
        .fetch_one(&state.pool)
        .await?;

    let ws: WsRow = sqlx::query_as(
        r#"SELECT w.id, w.kind, wm.role
           FROM workspaces w
           JOIN workspace_members wm ON wm.workspace_id = w.id
           WHERE wm.user_id = $1
           ORDER BY CASE WHEN w.kind = 'collector' THEN 0 ELSE 1 END, w.created_at ASC
           LIMIT 1"#,
    )
    .bind(row.user_id)
    .fetch_one(&state.pool)
    .await?;

    let (access_token, refresh_token) =
        issue_tokens(state, user.id, &user.email, ws.id, ws.kind, ws.role, Some(row.family_id)).await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
        user_id: user.id,
        email: user.email.clone(),
        name: user.name,
        workspace_id: ws.id,
    })
}

// ─── Logout ───────────────────────────────────────────────────────────────────

pub async fn logout(state: &AppState, raw_token: &str) -> Result<(), AppError> {
    let h = token_hash(raw_token);
    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(&h)
    .execute(&state.pool)
    .await?;
    Ok(())
}

// ─── Email verification ───────────────────────────────────────────────────────

pub async fn verify_email(state: &AppState, raw_token: &str) -> Result<(), AppError> {
    let h = token_hash(raw_token);
    let now = Utc::now();

    let row: TokenRow = sqlx::query_as(
        "SELECT id, user_id, expires_at, used_at FROM email_verifications WHERE token_hash = $1",
    )
    .bind(&h)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::BadRequest("invalid verification token".into()))?;

    if row.used_at.is_some() {
        return Err(AppError::BadRequest("token already used".into()));
    }
    if row.expires_at < now {
        return Err(AppError::BadRequest("token expired".into()));
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE email_verifications SET used_at = $1 WHERE id = $2")
        .bind(now)
        .bind(row.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE users SET email_verified_at = $1 WHERE id = $2")
        .bind(now)
        .bind(row.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

// ─── Password reset ───────────────────────────────────────────────────────────

pub async fn request_password_reset(state: &AppState, email: &str) -> Result<(), AppError> {
    let email = email.to_lowercase();
    let email = email.trim();
    let now = Utc::now();

    let user: Option<UserEmailRow> =
        sqlx::query_as("SELECT id, email FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&state.pool)
            .await?;

    if let Some(u) = user {
        let raw = generate_token();
        let h = token_hash(&raw);
        let expires_at = now + Duration::hours(PASSWORD_RESET_TTL_HOURS);
        sqlx::query(
            "INSERT INTO password_resets (id, user_id, token_hash, expires_at, created_at) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(Uuid::new_v4())
        .bind(u.id)
        .bind(&h)
        .bind(expires_at)
        .bind(now)
        .execute(&state.pool)
        .await?;
        let reset_url = format!("{}/auth/reset-password?token={}", state.base_url, raw);
        let _ =
            send_password_reset_email(&state.mailer, &email_from(), &u.email, &reset_url).await;
    }
    Ok(())
}

pub async fn confirm_password_reset(
    state: &AppState,
    raw_token: &str,
    new_password: &str,
) -> Result<(), AppError> {
    validate_password(new_password)?;
    let h = token_hash(raw_token);
    let now = Utc::now();

    let row: TokenRow = sqlx::query_as(
        "SELECT id, user_id, expires_at, used_at FROM password_resets WHERE token_hash = $1",
    )
    .bind(&h)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::BadRequest("invalid reset token".into()))?;

    if row.used_at.is_some() {
        return Err(AppError::BadRequest("reset token already used".into()));
    }
    if row.expires_at < now {
        return Err(AppError::BadRequest("reset token expired".into()));
    }

    let new_hash = hash_password(new_password)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(row.user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE password_resets SET used_at = $1 WHERE id = $2")
        .bind(now)
        .bind(row.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = $1 WHERE user_id = $2 AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(row.user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

// ─── Token issuance helper ────────────────────────────────────────────────────

async fn issue_tokens(
    state: &AppState,
    user_id: Uuid,
    email: &str,
    workspace_id: Uuid,
    workspace_kind: WorkspaceKind,
    role: MemberRole,
    family_id: Option<Uuid>,
) -> Result<(String, String), AppError> {
    let now = Utc::now();
    let secret = jwt_secret();
    let claims = Claims {
        sub: user_id,
        email: email.to_owned(),
        workspace_id,
        workspace_kind,
        role,
        iat: now.timestamp(),
        exp: (now + Duration::seconds(ACCESS_TOKEN_TTL_SECS)).timestamp(),
    };
    let access_token = encode_access_token(&claims, secret.as_bytes())?;

    let raw_rt = generate_token();
    let rt_hash = token_hash(&raw_rt);
    let family = family_id.unwrap_or_else(Uuid::new_v4);
    let rt_expires = now + Duration::days(REFRESH_TOKEN_TTL_DAYS);

    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, token_hash, family_id, expires_at, created_at) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&rt_hash)
    .bind(family)
    .bind(rt_expires)
    .bind(now)
    .execute(&state.pool)
    .await?;

    Ok((access_token, raw_rt))
}
