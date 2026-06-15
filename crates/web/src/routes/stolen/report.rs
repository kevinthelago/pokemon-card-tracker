//! Report-a-stolen-card form.
//!
//! Allows any authenticated user (seller or collector) to submit a stolen-card
//! report by grader + cert# with supporting evidence.  The report enters the
//! platform moderation queue (shared scope) or is immediately trusted within
//! the user's own workspace (private scope).

use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

// ─── Server functions ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportResult {
    pub id: String,
    pub status: String,
}

#[server(SubmitStolenReport, "/api")]
pub async fn submit_stolen_report(
    grader: String,
    cert_number: String,
    evidence: String,
    notes: String,
    scope: String, // "shared" | "private"
) -> Result<ReportResult, ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    let State(state): State<crate::app::AppState> = extract().await?;

    let endpoint = if scope == "private" {
        "/api/v1/stolen/private"
    } else {
        "/api/v1/stolen/reports"
    };

    let body = serde_json::json!({
        "grader": grader.trim().to_uppercase(),
        "cert_number": cert_number.trim(),
        "evidence": if evidence.trim().is_empty() { None::<String> } else { Some(evidence.trim().to_string()) },
        "notes": if notes.trim().is_empty() { None::<String> } else { Some(notes.trim().to_string()) },
    });

    let resp = state
        .http_client
        .post(format!("{}{}", state.api_base_url, endpoint))
        .json(&body)
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    if !resp.status().is_success() {
        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(ServerFnError::ServerError(
            err["error"]["message"]
                .as_str()
                .unwrap_or("Submission failed")
                .to_string(),
        ));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| ServerFnError::ServerError(e.to_string()))?;
    Ok(ReportResult {
        id: result["id"].as_str().unwrap_or("").to_string(),
        status: result["status"].as_str().unwrap_or("pending").to_string(),
    })
}

// ─── Component ────────────────────────────────────────────────────────────────

/// Stolen-card report submission form.
///
/// Supports two submission modes toggled by the user:
/// - **Community report** — enters the platform moderation queue; cert# only is
///   published if confirmed (no PII shared).
/// - **Private list** — immediately trusted within the user's own workspace only.
#[component]
pub fn ReportForm() -> impl IntoView {
    let submit = ServerAction::<SubmitStolenReport>::new();

    // Form field signals
    let grader = RwSignal::new(String::new());
    let cert_number = RwSignal::new(String::new());
    let evidence = RwSignal::new(String::new());
    let notes = RwSignal::new(String::new());
    let scope = RwSignal::new("shared".to_string());

    let pending = submit.pending();
    let result = submit.value();

    view! {
        <div class="max-w-2xl mx-auto p-6">
            <h1 class="text-2xl font-bold text-gray-900 mb-2">"Report a Stolen Card"</h1>
            <p class="text-sm text-gray-600 mb-6">
                "Reports for graded cards are cert-based and reliable.  "
                "Raw (ungraded) cards have no serial number and cannot be reliably matched in v1."
            </p>

            // Success state
            {move || {
                result.get().and_then(|r| r.ok()).map(|res| view! {
                    <div class="rounded-md bg-green-50 border border-green-200 p-4 mb-6">
                        <p class="text-green-800 font-medium">"Report submitted successfully."</p>
                        <p class="text-green-700 text-sm mt-1">
                            {if res.status == "pending" {
                                "Your report is now in the moderation queue.  You will be notified of any updates."
                            } else {
                                "The cert has been added to your private stolen list."
                            }}
                        </p>
                        <A
                            href="/stolen/my-reports"
                            attr:class="mt-2 inline-block text-sm text-green-700 underline"
                        >
                            "View my reports →"
                        </A>
                    </div>
                })
            }}

            // Error state
            {move || {
                result.get().and_then(|r| r.err()).map(|e| view! {
                    <div class="rounded-md bg-red-50 border border-red-200 p-4 mb-6">
                        <p class="text-red-800 font-medium">"Submission failed"</p>
                        <p class="text-red-700 text-sm mt-1">{e.to_string()}</p>
                    </div>
                })
            }}

            <ActionForm action=submit>
                // Scope toggle
                <div class="mb-6">
                    <fieldset>
                        <legend class="text-sm font-medium text-gray-700 mb-2">
                            "Report scope"
                        </legend>
                        <div class="flex gap-4">
                            <label class="flex items-center gap-2 cursor-pointer">
                                <input
                                    type="radio"
                                    name="scope"
                                    value="shared"
                                    checked=move || scope.get() == "shared"
                                    on:change=move |_| scope.set("shared".to_string())
                                    class="text-blue-600"
                                />
                                <span class="text-sm">
                                    <span class="font-medium">"Community report"</span>
                                    <span class="text-gray-500"> " — enters moderation queue"</span>
                                </span>
                            </label>
                            <label class="flex items-center gap-2 cursor-pointer">
                                <input
                                    type="radio"
                                    name="scope"
                                    value="private"
                                    checked=move || scope.get() == "private"
                                    on:change=move |_| scope.set("private".to_string())
                                    class="text-blue-600"
                                />
                                <span class="text-sm">
                                    <span class="font-medium">"Private (my workspace)"</span>
                                    <span class="text-gray-500"> " — trusted immediately"</span>
                                </span>
                            </label>
                        </div>
                    </fieldset>
                </div>

                // Grader
                <div class="mb-4">
                    <label for="grader" class="block text-sm font-medium text-gray-700 mb-1">
                        "Grader" <span class="text-red-500">"*"</span>
                    </label>
                    <select
                        id="grader"
                        name="grader"
                        required
                        class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                        on:change=move |e| grader.set(event_target_value(&e))
                    >
                        <option value="">"Select grader…"</option>
                        <option value="PSA">"PSA"</option>
                        <option value="CGC">"CGC"</option>
                        <option value="BGS">"BGS"</option>
                    </select>
                </div>

                // Cert number
                <div class="mb-4">
                    <label for="cert_number" class="block text-sm font-medium text-gray-700 mb-1">
                        "Certification number" <span class="text-red-500">"*"</span>
                    </label>
                    <input
                        id="cert_number"
                        name="cert_number"
                        type="text"
                        required
                        placeholder="e.g. 12345678"
                        class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                        prop:value=move || cert_number.get()
                        on:input=move |e| cert_number.set(event_target_value(&e))
                    />
                    <p class="mt-1 text-xs text-gray-500">
                        "Found on the cert label or the grading service website."
                    </p>
                </div>

                // Evidence
                <div class="mb-4">
                    <label for="evidence" class="block text-sm font-medium text-gray-700 mb-1">
                        "Evidence / description"
                    </label>
                    <textarea
                        id="evidence"
                        name="evidence"
                        rows="3"
                        placeholder="Describe how you know this card was stolen (original purchase proof, police report #, etc.)"
                        class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                        prop:value=move || evidence.get()
                        on:input=move |e| evidence.set(event_target_value(&e))
                    />
                </div>

                // Notes
                <div class="mb-6">
                    <label for="notes" class="block text-sm font-medium text-gray-700 mb-1">
                        "Additional notes"
                    </label>
                    <textarea
                        id="notes"
                        name="notes"
                        rows="2"
                        placeholder="Any other context for the moderator"
                        class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                        prop:value=move || notes.get()
                        on:input=move |e| notes.set(event_target_value(&e))
                    />
                </div>

                // Info box for community scope
                {move || (scope.get() == "shared").then(|| view! {
                    <div class="rounded-md bg-blue-50 border border-blue-200 p-3 mb-4 text-sm text-blue-800">
                        "Community reports are reviewed by platform moderators before being added to the shared list.  "
                        "Only the cert# and grader are published — your identity and evidence are never shared publicly."
                    </div>
                })}

                <div class="flex gap-3 items-center">
                    <button
                        type="submit"
                        disabled=move || pending.get()
                        class="bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white font-medium px-4 py-2 rounded-md text-sm transition-colors"
                    >
                        {move || if pending.get() { "Submitting…" } else { "Submit report" }}
                    </button>
                    <A href="/stolen/my-reports" attr:class="text-sm text-gray-600 hover:underline">
                        "View my reports"
                    </A>
                </div>
            </ActionForm>
        </div>
    }
}
