//! HTTP client helpers for the CardGuard API.

use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::routes::reconcile::{
    DiscrepancyDto, MappingQueueDto, MappingWithBackfillDto, ReconciliationReportDto,
    ReportDetailDto,
};
use crate::routes::risk::{
    FlagKind, FlagSeverity, FlagStatus, FlagsPage, Notification, RiskFlag, RiskSummary,
};
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

async fn patch_json_ret<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, String> {
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
    resp.json::<T>().await.map_err(|e| e.to_string())
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

// ─── Reconcile API ────────────────────────────────────────────────────────────

pub async fn fetch_reconcile_reports(
    workspace_id: Uuid,
) -> Result<Vec<ReconciliationReportDto>, String> {
    get_json(&format!("/workspaces/{workspace_id}/pos/reconciliation")).await
}

pub async fn fetch_reconcile_report(
    workspace_id: Uuid,
    report_id: Uuid,
) -> Result<ReportDetailDto, String> {
    get_json(&format!("/workspaces/{workspace_id}/pos/reconciliation/{report_id}")).await
}

pub async fn trigger_sync(
    workspace_id: Uuid,
    connection_id: Uuid,
) -> Result<ReconciliationReportDto, String> {
    #[derive(Serialize)]
    struct Body {
        connection_id: Uuid,
    }
    post_json(
        &format!("/workspaces/{workspace_id}/pos/sync"),
        &Body { connection_id },
    )
    .await
}

pub async fn resolve_discrepancy(
    workspace_id: Uuid,
    report_id: Uuid,
    disc_id: Uuid,
    resolution: &str,
    notes: Option<String>,
) -> Result<DiscrepancyDto, String> {
    #[derive(Serialize)]
    struct Body {
        resolution: String,
        notes: Option<String>,
    }
    post_json(
        &format!("/workspaces/{workspace_id}/pos/reconciliation/{report_id}/discrepancies/{disc_id}/resolve"),
        &Body { resolution: resolution.to_string(), notes },
    )
    .await
}

pub async fn fetch_mapping_queue(workspace_id: Uuid) -> Result<MappingQueueDto, String> {
    get_json(&format!("/workspaces/{workspace_id}/pos/mapping")).await
}

pub async fn create_mapping(
    workspace_id: Uuid,
    connection_id: Uuid,
    pos_sku: String,
    printing_id: Uuid,
) -> Result<MappingWithBackfillDto, String> {
    #[derive(Serialize)]
    struct Body {
        connection_id: Uuid,
        pos_sku: String,
        printing_id: Uuid,
    }
    post_json(
        &format!("/workspaces/{workspace_id}/pos/mapping"),
        &Body { connection_id, pos_sku, printing_id },
    )
    .await
}

pub async fn delete_mapping(workspace_id: Uuid, mapping_id: Uuid) -> Result<(), String> {
    delete_req(&format!("/workspaces/{workspace_id}/pos/mapping/{mapping_id}")).await
}

// ─── Stolen-card API ─────────────────────────────────────────────────────────

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
        &Body { grader, cert_number, evidence, notes },
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

// ─── Risk API ─────────────────────────────────────────────────────────────────

fn flag_kind_param(k: FlagKind) -> &'static str {
    match k {
        FlagKind::StolenCard => "stolen_card",
        FlagKind::Scalper => "scalper",
        FlagKind::Counterfeit => "counterfeit",
    }
}

fn flag_severity_param(s: FlagSeverity) -> &'static str {
    match s {
        FlagSeverity::Low => "low",
        FlagSeverity::Medium => "medium",
        FlagSeverity::High => "high",
        FlagSeverity::Critical => "critical",
    }
}

fn flag_status_param(s: FlagStatus) -> &'static str {
    match s {
        FlagStatus::Open => "open",
        FlagStatus::Reviewed => "reviewed",
        FlagStatus::Dismissed => "dismissed",
    }
}

pub async fn fetch_risk_flags(
    workspace_id: Uuid,
    kind: Option<FlagKind>,
    severity: Option<FlagSeverity>,
    status: Option<FlagStatus>,
    after: Option<String>,
) -> Result<FlagsPage, String> {
    let mut qs = String::new();
    if let Some(k) = kind {
        qs.push_str(&format!("&kind={}", flag_kind_param(k)));
    }
    if let Some(s) = severity {
        qs.push_str(&format!("&severity={}", flag_severity_param(s)));
    }
    if let Some(s) = status {
        qs.push_str(&format!("&status={}", flag_status_param(s)));
    }
    if let Some(c) = after {
        qs.push_str(&format!("&after={c}"));
    }
    let qs = if qs.is_empty() { String::new() } else { format!("?{}", &qs[1..]) };
    get_json(&format!("/workspaces/{workspace_id}/risk/flags{qs}")).await
}

pub async fn fetch_risk_flag(workspace_id: Uuid, flag_id: Uuid) -> Result<RiskFlag, String> {
    get_json(&format!("/workspaces/{workspace_id}/risk/flags/{flag_id}")).await
}

pub async fn triage_flag(
    workspace_id: Uuid,
    flag_id: Uuid,
    status: &str,
) -> Result<RiskFlag, String> {
    patch_json_ret(
        &format!("/workspaces/{workspace_id}/risk/flags/{flag_id}"),
        &serde_json::json!({ "status": status }),
    )
    .await
}

pub async fn bulk_triage_flags(
    workspace_id: Uuid,
    flag_ids: &[Uuid],
    status: &str,
) -> Result<(), String> {
    let body = serde_json::json!({ "flag_ids": flag_ids, "status": status });
    let resp = Request::post(&format!("{API_BASE}/workspaces/{workspace_id}/risk/flags/bulk"))
        .json(&body)
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

pub async fn fetch_risk_summary(workspace_id: Uuid) -> Result<RiskSummary, String> {
    get_json(&format!("/workspaces/{workspace_id}/risk/summary")).await
}

pub async fn fetch_notifications(workspace_id: Uuid) -> Result<Vec<Notification>, String> {
    get_json(&format!("/workspaces/{workspace_id}/notifications")).await
}

pub async fn mark_notification_read(workspace_id: Uuid, notif_id: Uuid) -> Result<(), String> {
    let resp = Request::post(&format!(
        "{API_BASE}/workspaces/{workspace_id}/notifications/{notif_id}/read"
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", resp.status(), text));
    }
    Ok(())
}
