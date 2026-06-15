use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::CookieJar;
use cookie::{Cookie, SameSite};
use std::collections::HashMap;

use crate::{app::AppState, error::AppError};

use super::{
    models::{
        LoginRequest, PasswordResetConfirm, PasswordResetRequest, RefreshRequest, RegisterRequest,
    },
    service,
};

const REFRESH_COOKIE_NAME: &str = "rt";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/verify-email", get(verify_email))
        .route("/auth/password-reset", post(request_password_reset))
        .route("/auth/password-reset/confirm", post(confirm_password_reset))
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let resp = service::register(&state, &body.email, &body.name, &body.password).await?;
    let cookie = build_rt_cookie(&resp.refresh_token);
    Ok((
        StatusCode::CREATED,
        set_cookie_headers(cookie),
        Json(serde_json::json!({
            "access_token": resp.access_token,
            "user_id": resp.user_id,
            "email": resp.email,
            "name": resp.name,
            "workspace_id": resp.workspace_id,
        })),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let resp = service::login(&state, &body.email, &body.password).await?;
    let cookie = build_rt_cookie(&resp.refresh_token);
    Ok((
        StatusCode::OK,
        set_cookie_headers(cookie),
        Json(serde_json::json!({
            "access_token": resp.access_token,
            "user_id": resp.user_id,
            "email": resp.email,
            "name": resp.name,
            "workspace_id": resp.workspace_id,
        })),
    ))
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Option<Json<RefreshRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let raw = jar
        .get(REFRESH_COOKIE_NAME)
        .map(|c| c.value().to_owned())
        .or_else(|| body.map(|b| b.refresh_token.clone()))
        .ok_or(AppError::Unauthorized)?;

    let resp = service::refresh(&state, &raw).await?;
    let cookie = build_rt_cookie(&resp.refresh_token);
    Ok((
        StatusCode::OK,
        set_cookie_headers(cookie),
        Json(serde_json::json!({
            "access_token": resp.access_token,
            "user_id": resp.user_id,
            "email": resp.email,
            "name": resp.name,
            "workspace_id": resp.workspace_id,
        })),
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Option<Json<RefreshRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let raw = jar
        .get(REFRESH_COOKIE_NAME)
        .map(|c| c.value().to_owned())
        .or_else(|| body.map(|b| b.refresh_token.clone()));

    if let Some(token) = raw {
        service::logout(&state, &token).await?;
    }

    let clear = Cookie::build((REFRESH_COOKIE_NAME, ""))
        .path("/api/auth")
        .max_age(time::Duration::seconds(0))
        .http_only(true)
        .build();

    Ok((
        StatusCode::NO_CONTENT,
        set_cookie_headers(clear.to_string()),
    ))
}

async fn verify_email(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let token = params
        .get("token")
        .ok_or_else(|| AppError::BadRequest("missing token".into()))?;
    service::verify_email(&state, token).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn request_password_reset(
    State(state): State<AppState>,
    Json(body): Json<PasswordResetRequest>,
) -> Result<impl IntoResponse, AppError> {
    service::request_password_reset(&state, &body.email).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(body): Json<PasswordResetConfirm>,
) -> Result<impl IntoResponse, AppError> {
    service::confirm_password_reset(&state, &body.token, &body.new_password).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn build_rt_cookie(token: &str) -> String {
    Cookie::build((REFRESH_COOKIE_NAME, token.to_owned()))
        .path("/api/auth")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .max_age(time::Duration::days(service::REFRESH_TOKEN_TTL_DAYS))
        .build()
        .to_string()
}

fn set_cookie_headers(cookie_str: String) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::SET_COOKIE,
        cookie_str.parse().expect("valid cookie header"),
    );
    h
}
