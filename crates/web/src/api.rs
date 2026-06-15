//! HTTP client helpers for the CardGuard API.

use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::routes::settings::team::{
    ChangeRoleBody, InviteDto, SendInviteBody, TeamResponse,
};

const API_BASE: &str = "/api";

async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = Request::get(&format!("{API_BASE}{path}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

async fn post_json<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let resp = Request::post(&format!("{API_BASE}{path}"))
        .json(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

async fn patch_json<B: Serialize>(path: &str, body: &B) -> Result<(), String> {
    let resp = Request::patch(&format!("{API_BASE}{path}"))
        .json(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    Ok(())
}

async fn delete_req(path: &str) -> Result<(), String> {
    let resp = Request::delete(&format!("{API_BASE}{path}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    Ok(())
}

// ─── Team API ────────────────────────────────────────────────────────────────

pub async fn fetch_team(workspace_id: Uuid) -> Result<TeamResponse, String> {
    get_json(&format!("/workspaces/{workspace_id}/team")).await
}

pub async fn send_invite(workspace_id: Uuid, body: &SendInviteBody) -> Result<InviteDto, String> {
    post_json(&format!("/workspaces/{workspace_id}/invites"), body).await
}

pub async fn resend_invite(workspace_id: Uuid, invite_id: Uuid) -> Result<InviteDto, String> {
    post_json(
        &format!("/workspaces/{workspace_id}/invites/{invite_id}/resend"),
        &serde_json::Value::Null,
    )
    .await
}

pub async fn cancel_invite(workspace_id: Uuid, invite_id: Uuid) -> Result<(), String> {
    delete_req(&format!("/workspaces/{workspace_id}/invites/{invite_id}")).await
}

pub async fn change_member_role(
    workspace_id: Uuid,
    user_id: Uuid,
    body: &ChangeRoleBody,
) -> Result<(), String> {
    patch_json(
        &format!("/workspaces/{workspace_id}/members/{user_id}"),
        body,
    )
    .await
}

pub async fn remove_member(workspace_id: Uuid, user_id: Uuid) -> Result<(), String> {
    delete_req(&format!("/workspaces/{workspace_id}/members/{user_id}")).await
}
