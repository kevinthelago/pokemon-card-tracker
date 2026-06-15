//! HTTP client helpers for the CardGuard API.

use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::routes::settings::team::{ChangeRoleBody, InviteDto, SendInviteBody, TeamResponse};
use crate::routes::stolen::dispute::ReportDetails;
use crate::routes::stolen::my_reports::ReportSummary;
use crate::routes::stolen::queue::QueueReport;

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

// ─── Stolen-card API ─────────────────────────────────────────────────────────

/// Submit a new community stolen-card report. Returns the created report ID.
pub async fn submit_stolen_report(
    grader: String,
    cert_number: String,
    evidence: Option<String>,
    notes: Option<String>,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct Body {
        grader: String,
        cert_number: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        evidence: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Created {
        id: uuid::Uuid,
    }

    let created: Created = post_json(
        "/stolen/reports",
        &Body {
            grader,
            cert_number,
            evidence,
            notes,
        },
    )
    .await?;
    Ok(created.id.to_string())
}

pub async fn fetch_my_reports() -> Result<Vec<ReportSummary>, String> {
    #[derive(serde::Deserialize)]
    struct Page {
        items: Vec<ReportSummary>,
    }
    let page: Page = get_json("/stolen/reports").await?;
    Ok(page.items)
}

pub async fn fetch_stolen_report(id: String) -> Result<ReportDetails, String> {
    get_json(&format!("/stolen/reports/{id}")).await
}

pub async fn fetch_moderator_queue() -> Result<Vec<QueueReport>, String> {
    #[derive(serde::Deserialize)]
    struct Page {
        items: Vec<QueueReport>,
    }
    let page: Page = get_json("/stolen/queue").await?;
    Ok(page.items)
}

pub async fn confirm_stolen_report(id: String, notes: Option<String>) -> Result<(), String> {
    #[derive(Serialize)]
    struct Body {
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    }
    post_json::<_, serde_json::Value>(&format!("/stolen/reports/{id}/confirm"), &Body { notes })
        .await
        .map(|_| ())
}

pub async fn reject_stolen_report(id: String, notes: Option<String>) -> Result<(), String> {
    #[derive(Serialize)]
    struct Body {
        #[serde(skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    }
    post_json::<_, serde_json::Value>(&format!("/stolen/reports/{id}/reject"), &Body { notes })
        .await
        .map(|_| ())
}

pub async fn submit_dispute(id: String, reason: String) -> Result<(), String> {
    #[derive(Serialize)]
    struct Body {
        reason: String,
    }
    post_json::<_, serde_json::Value>(&format!("/stolen/reports/{id}/dispute"), &Body { reason })
        .await
        .map(|_| ())
}
