use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use gloo_net::http::Request;

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionUser {
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub workspace_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthApiResponse {
    pub access_token: String,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub workspace_id: Uuid,
}

// ─── Reactive session atoms ───────────────────────────────────────────────────
// Access token lives in memory only (never localStorage) to minimize XSS exposure.
// Refresh token is an httpOnly cookie set by the server.

#[derive(Clone, Copy)]
pub struct Session(RwSignal<Option<SessionUser>>);

impl Session {
    pub fn new() -> Self {
        Session(RwSignal::new(None))
    }

    pub fn get(&self) -> Option<SessionUser> {
        self.0.get()
    }

    pub fn set_user(&self, user: SessionUser) {
        self.0.set(Some(user));
    }

    pub fn clear(&self) {
        self.0.set(None);
    }

    pub fn is_authenticated(&self) -> bool {
        self.0.with(|s| s.is_some())
    }
}

#[derive(Clone, Copy)]
pub struct AccessToken(RwSignal<Option<String>>);

impl AccessToken {
    pub fn new() -> Self {
        AccessToken(RwSignal::new(None))
    }

    pub fn get(&self) -> Option<String> {
        self.0.get()
    }

    pub fn set(&self, token: String) {
        self.0.set(Some(token));
    }

    pub fn clear(&self) {
        self.0.set(None);
    }
}

// ─── Context providers ────────────────────────────────────────────────────────

pub fn provide_auth(session: Session, access_token: AccessToken) {
    provide_context(session);
    provide_context(access_token);
}

pub fn use_session() -> Session {
    use_context::<Session>().expect("Session not provided")
}

pub fn use_access_token() -> AccessToken {
    use_context::<AccessToken>().expect("AccessToken not provided")
}

// ─── API calls ────────────────────────────────────────────────────────────────

const API: &str = "/api";

pub async fn api_login(email: &str, password: &str) -> Result<AuthApiResponse, String> {
    let resp = Request::post(&format!("{API}/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Login failed")
            .to_owned());
    }

    resp.json::<AuthApiResponse>().await.map_err(|e| e.to_string())
}

pub async fn api_register(
    email: &str,
    name: &str,
    password: &str,
) -> Result<AuthApiResponse, String> {
    let resp = Request::post(&format!("{API}/auth/register"))
        .json(&serde_json::json!({ "email": email, "name": name, "password": password }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Registration failed")
            .to_owned());
    }

    resp.json::<AuthApiResponse>().await.map_err(|e| e.to_string())
}

pub async fn api_logout() {
    let _ = Request::post(&format!("{API}/auth/logout"))
        .send()
        .await;
}

pub async fn api_refresh() -> Result<AuthApiResponse, String> {
    let resp = Request::post(&format!("{API}/auth/refresh"))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err("Session expired".into());
    }
    resp.json::<AuthApiResponse>().await.map_err(|e| e.to_string())
}
