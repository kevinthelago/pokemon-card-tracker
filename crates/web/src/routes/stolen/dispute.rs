//! Dispute page — allows the accused owner to challenge a stolen-card report.
//!
//! Accessed via a deep-link from a risk-flag notification or directly via
//! `/stolen/dispute/:id`.  The disputing owner provides their counter-claim;
//! no automatic action is taken and no flag is raised against the reporter
//! (the moderator adjudicates).

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};
use serde::{Deserialize, Serialize};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDetails {
    pub id: String,
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub evidence: Option<String>,
    pub dispute_notes: Option<String>,
    pub confirmed_at: Option<String>,
    pub created_at: String,
}

// ─── Server functions ─────────────────────────────────────────────────────────

#[server(FetchReport, "/api")]
pub async fn fetch_report(id: String) -> Result<ReportDetails, ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    let State(state): State<crate::app::AppState> = extract().await?;

    let resp = state
        .http_client
        .get(format!("{}/api/v1/stolen/reports/{}", state.api_base_url, id))
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    if resp.status() == 404 {
        return Err(ServerFnError::ServerError("Report not found.".into()));
    }
    if resp.status() == 403 {
        return Err(ServerFnError::ServerError(
            "You do not have access to this report.".into(),
        ));
    }

    let details: ReportDetails = resp
        .json()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    Ok(details)
}

#[server(SubmitDispute, "/api")]
pub async fn submit_dispute(id: String, notes: String) -> Result<(), ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    if notes.trim().is_empty() {
        return Err(ServerFnError::ServerError(
            "Dispute notes are required.".into(),
        ));
    }

    let State(state): State<crate::app::AppState> = extract().await?;

    let resp = state
        .http_client
        .post(format!(
            "{}/api/v1/stolen/reports/{}/dispute",
            state.api_base_url, id
        ))
        .json(&serde_json::json!({ "notes": notes.trim() }))
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(ServerFnError::ServerError(
            err["error"]["message"]
                .as_str()
                .unwrap_or("Dispute submission failed")
                .to_string(),
        ));
    }
    Ok(())
}

// ─── Component ────────────────────────────────────────────────────────────────

/// Dispute page for an accused card owner.
///
/// Renders differently based on report status:
/// - `pending` / `confirmed` — dispute form available
/// - `disputed` — already disputed; shows the registered dispute notes
/// - `rejected` / `resolved` — no action needed
#[component]
pub fn DisputePage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").cloned().unwrap_or_default());

    let report = Resource::new(id, |id| fetch_report(id));
    let dispute = ServerAction::<SubmitDispute>::new();
    let notes = RwSignal::new(String::new());
    let pending = dispute.pending();
    let dispute_result = dispute.value();

    view! {
        <div class="max-w-2xl mx-auto p-6">
            <h1 class="text-2xl font-bold text-gray-900 mb-2">"Dispute a Report"</h1>
            <p class="text-sm text-gray-600 mb-6">
                "If you believe this stolen-card report is inaccurate, submit a dispute.  \
                 No automatic action is taken; a moderator will review both sides and adjudicate."
            </p>

            <Suspense fallback=move || view! { <DisputeLoading /> }>
                {move || {
                    report.get().map(|result| match result {
                        Err(e) => view! {
                            <div class="rounded-md bg-red-50 border border-red-200 p-4">
                                <p class="text-red-800 font-medium">"Could not load report"</p>
                                <p class="text-red-700 text-sm mt-1">{e.to_string()}</p>
                                <A href="/stolen/my-reports" attr:class="mt-2 inline-block text-sm text-red-700 underline">
                                    "← My reports"
                                </A>
                            </div>
                        }.into_any(),

                        Ok(r) => {
                            view! { <DisputeContent report=r dispute=dispute notes=notes pending=pending dispute_result=dispute_result /> }
                                .into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn DisputeContent(
    report: ReportDetails,
    dispute: ServerAction<SubmitDispute>,
    notes: RwSignal<String>,
    pending: Signal<bool>,
    dispute_result: Signal<Option<Result<(), ServerFnError>>>,
) -> impl IntoView {
    let report_id = report.id.clone();

    view! {
        // Report summary card
        <div class="border border-gray-200 rounded-lg p-4 bg-gray-50 mb-6">
            <div class="flex items-center justify-between mb-2">
                <span class="font-mono font-medium text-gray-900">
                    {format!("{} — {}", report.grader, report.cert_number)}
                </span>
                {status_badge_view(&report.status)}
            </div>
            {report.evidence.as_ref().map(|e| view! {
                <p class="text-sm text-gray-600 mt-1">"Evidence: " {e.clone()}</p>
            })}
            <p class="text-xs text-gray-400 mt-2">"Reported: " {report.created_at.clone()}</p>
        </div>

        // Already disputed state
        {report.dispute_notes.as_ref().map(|d| view! {
            <div class="rounded-md bg-amber-50 border border-amber-200 p-4 mb-6">
                <p class="text-amber-800 font-medium mb-1">"Dispute already on file"</p>
                <p class="text-amber-700 text-sm">{d.clone()}</p>
                <p class="text-amber-600 text-xs mt-2">
                    "A moderator will review this dispute.  You will be notified of the outcome."
                </p>
            </div>
        })}

        // Dispute form (only available for disputeable states and if not already disputed)
        {(report.dispute_notes.is_none()
            && matches!(report.status.as_str(), "pending" | "confirmed")).then(|| {

            // Success state after submission
            let submitted = move || dispute_result.get().and_then(|r| r.ok()).is_some();

            view! {
                <div>
                    {move || submitted().then(|| view! {
                        <div class="rounded-md bg-green-50 border border-green-200 p-4 mb-4">
                            <p class="text-green-800 font-medium">"Dispute submitted"</p>
                            <p class="text-green-700 text-sm mt-1">
                                "A moderator will review your dispute and the original report.  \
                                 The report is now marked as disputed — no automatic action will be taken."
                            </p>
                        </div>
                    })}

                    {move || dispute_result.get().and_then(|r| r.err()).map(|e| view! {
                        <div class="rounded-md bg-red-50 border border-red-200 p-4 mb-4">
                            <p class="text-red-800 font-medium">"Submission failed"</p>
                            <p class="text-red-700 text-sm mt-1">{e.to_string()}</p>
                        </div>
                    })}

                    {move || (!submitted()).then(|| view! {
                        <ActionForm action=dispute>
                            <input type="hidden" name="id" value=report_id.clone() />

                            <div class="mb-4">
                                <label for="notes" class="block text-sm font-medium text-gray-700 mb-1">
                                    "Your counter-claim" <span class="text-red-500">"*"</span>
                                </label>
                                <textarea
                                    id="notes"
                                    name="notes"
                                    rows="4"
                                    required
                                    placeholder="Explain why you believe this report is inaccurate — include any proof of legitimate ownership, purchase receipts, or other relevant details."
                                    class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                                    prop:value=move || notes.get()
                                    on:input=move |e| notes.set(event_target_value(&e))
                                />
                                <p class="mt-1 text-xs text-gray-500">
                                    "Your dispute notes are shared only with platform moderators, not publicly."
                                </p>
                            </div>

                            <div class="rounded-md bg-blue-50 border border-blue-200 p-3 mb-4 text-sm text-blue-800">
                                "Submitting a dispute does not remove the report — a moderator reviews \
                                 both sides and makes the final decision.  Filing a false dispute is \
                                 recorded in the audit log."
                            </div>

                            <div class="flex gap-3 items-center">
                                <button
                                    type="submit"
                                    disabled=move || pending.get()
                                    class="bg-orange-600 hover:bg-orange-700 disabled:bg-orange-400 text-white font-medium px-4 py-2 rounded-md text-sm transition-colors"
                                >
                                    {move || if pending.get() { "Submitting…" } else { "Submit dispute" }}
                                </button>
                                <A href="/stolen/my-reports" attr:class="text-sm text-gray-600 hover:underline">
                                    "Cancel"
                                </A>
                            </div>
                        </ActionForm>
                    })}
                </div>
            }
        })}

        // Non-disputeable states
        {(!matches!(report.status.as_str(), "pending" | "confirmed")
            && report.dispute_notes.is_none()).then(|| view! {
            <div class="rounded-md bg-gray-50 border border-gray-200 p-4">
                <p class="text-gray-700">
                    {match report.status.as_str() {
                        "rejected" => "This report was rejected — no action is needed.",
                        "resolved" => "This report has been marked resolved (card recovered).",
                        _ => "This report cannot be disputed in its current state.",
                    }}
                </p>
                <A href="/stolen/my-reports" attr:class="mt-2 inline-block text-sm text-gray-600 underline">
                    "← Back to my reports"
                </A>
            </div>
        })}
    }
}

// ─── Loading state ─────────────────────────────────────────────────────────────

#[component]
fn DisputeLoading() -> impl IntoView {
    view! {
        <div class="animate-pulse space-y-4">
            <div class="border border-gray-200 rounded-lg p-4 bg-gray-50">
                <div class="h-4 bg-gray-200 rounded w-1/3 mb-2" />
                <div class="h-3 bg-gray-100 rounded w-2/3" />
            </div>
            <div class="h-24 bg-gray-100 rounded" />
            <div class="h-10 bg-gray-200 rounded w-32" />
        </div>
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn status_badge_view(status: &str) -> impl IntoView {
    let (class, text) = match status {
        "pending"   => ("bg-amber-100 text-amber-800",  "Pending"),
        "confirmed" => ("bg-green-100 text-green-800",  "Confirmed"),
        "disputed"  => ("bg-orange-100 text-orange-800","Disputed"),
        "rejected"  => ("bg-red-100 text-red-800",      "Rejected"),
        "resolved"  => ("bg-gray-100 text-gray-700",    "Resolved"),
        _           => ("bg-gray-100 text-gray-700",    "Unknown"),
    };
    view! {
        <span class=format!("inline-flex items-center px-2 py-0.5 rounded text-xs font-medium {class}")>
            {text}
        </span>
    }
}
