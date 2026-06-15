//! Platform-moderator moderation queue for stolen-card reports.
//!
//! Visible ONLY to users with the `is_platform_moderator` claim.  Displays
//! pending and disputed reports and allows inline confirm / reject actions.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueReport {
    pub id: String,
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub evidence: Option<String>,
    pub notes: Option<String>,
    pub dispute_notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuePage {
    pub data: Vec<QueueReport>,
    pub next_cursor: Option<String>,
}

// ─── Server functions ─────────────────────────────────────────────────────────

#[server(FetchQueue, "/api")]
pub async fn fetch_queue(cursor: Option<String>) -> Result<QueuePage, ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    let State(state): State<crate::app::AppState> = extract().await?;

    let mut url = format!("{}/api/v1/stolen/queue?limit=25", state.api_base_url);
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={c}"));
    }

    let resp = state
        .http_client
        .get(&url)
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    if resp.status() == 403 {
        return Err(ServerFnError::ServerError(
            "You do not have moderator access.".into(),
        ));
    }

    let page: QueuePage = resp
        .json()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    Ok(page)
}

#[server(ConfirmReport, "/api")]
pub async fn confirm_report(id: String, notes: String) -> Result<(), ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    let State(state): State<crate::app::AppState> = extract().await?;

    let resp = state
        .http_client
        .post(format!("{}/api/v1/stolen/reports/{}/confirm", state.api_base_url, id))
        .json(&serde_json::json!({ "notes": if notes.trim().is_empty() { None::<String> } else { Some(notes.trim().to_string()) } }))
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(ServerFnError::ServerError(
            err["error"]["message"]
                .as_str()
                .unwrap_or("Action failed")
                .to_string(),
        ));
    }
    Ok(())
}

#[server(RejectReport, "/api")]
pub async fn reject_report(id: String, notes: String) -> Result<(), ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    let State(state): State<crate::app::AppState> = extract().await?;

    let resp = state
        .http_client
        .post(format!("{}/api/v1/stolen/reports/{}/reject", state.api_base_url, id))
        .json(&serde_json::json!({ "notes": if notes.trim().is_empty() { None::<String> } else { Some(notes.trim().to_string()) } }))
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(ServerFnError::ServerError(
            err["error"]["message"]
                .as_str()
                .unwrap_or("Action failed")
                .to_string(),
        ));
    }
    Ok(())
}

// ─── Component ────────────────────────────────────────────────────────────────

/// Platform-moderator queue — lists pending and disputed reports for review.
#[component]
pub fn ModeratorQueue() -> impl IntoView {
    let queue = Resource::new(|| (), |_| fetch_queue(None));
    let on_action = move || queue.refetch();

    view! {
        <div class="max-w-4xl mx-auto p-6">
            <h1 class="text-2xl font-bold text-gray-900 mb-2">"Moderation Queue"</h1>
            <p class="text-sm text-gray-600 mb-6">
                "Review pending stolen-card reports.  Confirmed reports are added to the community \
                 stolen-cert list; rejected reports are removed with an audit record."
            </p>

            <Suspense fallback=move || view! { <QueueLoading /> }>
                {move || {
                    queue.get().map(|result| match result {
                        Err(e) if e.to_string().contains("moderator") => {
                            view! { <AccessDenied /> }.into_any()
                        }
                        Err(e) => view! {
                            <div class="rounded-md bg-red-50 border border-red-200 p-4">
                                <p class="text-red-800">"Failed to load queue: " {e.to_string()}</p>
                            </div>
                        }.into_any(),
                        Ok(page) if page.data.is_empty() => {
                            view! { <QueueEmpty /> }.into_any()
                        }
                        Ok(page) => {
                            view! {
                                <div class="space-y-4">
                                    <p class="text-sm text-gray-500">
                                        {page.data.len()} " report(s) pending review"
                                    </p>
                                    <For
                                        each=move || page.data.clone()
                                        key=|r| r.id.clone()
                                        children=move |report| {
                                            view! {
                                                <QueueItem
                                                    report=report.clone()
                                                    on_action=on_action
                                                />
                                            }
                                        }
                                    />
                                </div>
                            }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn QueueItem(report: QueueReport, on_action: impl Fn() + 'static + Clone) -> impl IntoView {
    let confirm_action = ServerAction::<ConfirmReport>::new();
    let reject_action = ServerAction::<RejectReport>::new();

    let moderator_notes = RwSignal::new(String::new());
    let expanded = RwSignal::new(false);

    let report_id = report.id.clone();
    let on_action_clone = on_action.clone();

    // Refetch queue after any action completes
    let confirm_result = confirm_action.value();
    let reject_result = reject_action.value();

    let watch_actions = move || {
        if confirm_result.get().is_some() || reject_result.get().is_some() {
            on_action_clone();
        }
    };

    view! {
        {watch_actions}
        <div class="border border-gray-200 rounded-lg bg-white shadow-sm overflow-hidden">
            // Header row
            <div class="p-4 flex items-start justify-between gap-4">
                <div class="flex-1">
                    <div class="flex items-center gap-3 mb-1">
                        <span class="font-mono font-medium text-gray-900">
                            {format!("{} — {}", report.grader, report.cert_number)}
                        </span>
                        {if report.status == "disputed" {
                            Some(view! {
                                <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-orange-100 text-orange-800">
                                    "Disputed"
                                </span>
                            })
                        } else {
                            None
                        }}
                    </div>
                    <p class="text-xs text-gray-400">"Submitted: " {report.created_at.clone()}</p>
                </div>

                <button
                    class="text-sm text-blue-600 hover:underline"
                    on:click=move |_| expanded.update(|v| *v = !*v)
                >
                    {move || if expanded.get() { "Collapse ↑" } else { "View details ↓" }}
                </button>
            </div>

            // Expandable details
            {move || expanded.get().then(|| view! {
                <div class="border-t border-gray-100 px-4 py-3 bg-gray-50 space-y-3">
                    {report.evidence.as_ref().map(|e| view! {
                        <div>
                            <p class="text-xs font-medium text-gray-500 uppercase tracking-wide mb-1">"Evidence"</p>
                            <p class="text-sm text-gray-700">{e.clone()}</p>
                        </div>
                    })}

                    {report.notes.as_ref().map(|n| view! {
                        <div>
                            <p class="text-xs font-medium text-gray-500 uppercase tracking-wide mb-1">"Reporter notes"</p>
                            <p class="text-sm text-gray-700">{n.clone()}</p>
                        </div>
                    })}

                    {report.dispute_notes.as_ref().map(|d| view! {
                        <div class="rounded bg-orange-50 border border-orange-200 p-3">
                            <p class="text-xs font-medium text-orange-700 uppercase tracking-wide mb-1">"Dispute notes"</p>
                            <p class="text-sm text-orange-800">{d.clone()}</p>
                        </div>
                    })}

                    // Moderator notes input
                    <div>
                        <label class="text-xs font-medium text-gray-500 uppercase tracking-wide mb-1 block">
                            "Moderator notes (optional)"
                        </label>
                        <textarea
                            rows="2"
                            placeholder="Internal notes for the audit log…"
                            class="w-full border border-gray-300 rounded px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                            prop:value=move || moderator_notes.get()
                            on:input=move |e| moderator_notes.set(event_target_value(&e))
                        />
                    </div>

                    // Action buttons
                    <div class="flex gap-3">
                        <ActionForm action=confirm_action>
                            <input type="hidden" name="id" value=report_id.clone() />
                            <input type="hidden" name="notes" prop:value=move || moderator_notes.get() />
                            <button
                                type="submit"
                                disabled=move || confirm_action.pending().get()
                                class="bg-green-600 hover:bg-green-700 disabled:bg-green-400 text-white text-sm font-medium px-4 py-2 rounded-md transition-colors"
                            >
                                {move || if confirm_action.pending().get() { "Confirming…" } else { "✓ Confirm" }}
                            </button>
                        </ActionForm>

                        <ActionForm action=reject_action>
                            <input type="hidden" name="id" value=report_id.clone() />
                            <input type="hidden" name="notes" prop:value=move || moderator_notes.get() />
                            <button
                                type="submit"
                                disabled=move || reject_action.pending().get()
                                class="bg-red-600 hover:bg-red-700 disabled:bg-red-400 text-white text-sm font-medium px-4 py-2 rounded-md transition-colors"
                            >
                                {move || if reject_action.pending().get() { "Rejecting…" } else { "✗ Reject" }}
                            </button>
                        </ActionForm>
                    </div>

                    // Inline action errors
                    {move || confirm_action.value().get().and_then(|r| r.err()).map(|e| view! {
                        <p class="text-red-700 text-sm">"Confirm failed: " {e.to_string()}</p>
                    })}
                    {move || reject_action.value().get().and_then(|r| r.err()).map(|e| view! {
                        <p class="text-red-700 text-sm">"Reject failed: " {e.to_string()}</p>
                    })}
                </div>
            })}
        </div>
    }
}

// ─── Empty / loading / access-denied states ───────────────────────────────────

#[component]
fn QueueEmpty() -> impl IntoView {
    view! {
        <div class="text-center py-16 text-gray-500">
            <svg class="mx-auto mb-4 h-12 w-12 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <p class="text-lg font-medium text-gray-900 mb-1">"Queue is clear"</p>
            <p class="text-sm text-gray-500">"No pending reports — all caught up."</p>
        </div>
    }
}

#[component]
fn QueueLoading() -> impl IntoView {
    view! {
        <div class="space-y-3">
            {(0..4).map(|_| view! {
                <div class="border border-gray-200 rounded-lg p-4 bg-white animate-pulse">
                    <div class="h-4 bg-gray-200 rounded w-1/3 mb-2" />
                    <div class="h-3 bg-gray-100 rounded w-1/4" />
                </div>
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn AccessDenied() -> impl IntoView {
    view! {
        <div class="rounded-md bg-red-50 border border-red-200 p-6 text-center">
            <p class="text-red-900 font-medium text-lg mb-1">"Access denied"</p>
            <p class="text-red-700 text-sm">"The moderation queue is only accessible to platform moderators."</p>
        </div>
    }
}
