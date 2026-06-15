//! Dispute page — allows the accused card owner to challenge a stolen-card report (CSR).

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};
use serde::{Deserialize, Serialize};

use crate::api;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDetails {
    pub id: String,
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub evidence: Option<String>,
    pub created_at: String,
}

#[component]
pub fn DisputePage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").as_deref().unwrap_or_default().to_string());

    let report = LocalResource::new(move || {
        let id = id();
        async move { api::fetch_stolen_report(id).await }
    });

    let reason = RwSignal::new(String::new());
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (submitted, set_submitted) = signal(false);

    view! {
        <div class="max-w-2xl mx-auto p-6">
            <h1 class="text-2xl font-bold text-gray-900 mb-2">"Dispute a Report"</h1>
            <p class="text-sm text-gray-600 mb-6">
                "If you believe this stolen-card report is inaccurate, submit a dispute. \
                 No automatic action is taken; a moderator will review both sides and adjudicate."
            </p>

            <Suspense fallback=move || view! {
                <div class="animate-pulse space-y-4">
                    <div class="border border-gray-200 rounded-lg p-4 bg-gray-50">
                        <div class="h-4 bg-gray-200 rounded w-1/3 mb-2" />
                        <div class="h-3 bg-gray-100 rounded w-2/3" />
                    </div>
                </div>
            }>
                {move || {
                    report.get().as_deref().map(|result| match result {
                        Err(e) => {
                            let e = e.clone();
                            view! {
                                <div class="rounded-md bg-red-50 border border-red-200 p-4">
                                    <p class="text-red-800 font-medium">"Could not load report"</p>
                                    <p class="text-red-700 text-sm mt-1">{e}</p>
                                    <A href="/stolen/my-reports" attr:class="mt-2 inline-block text-sm text-red-700 underline">
                                        "← My reports"
                                    </A>
                                </div>
                            }.into_any()
                        }

                        Ok(r) => {
                            let r = r.clone();
                            let report_id = r.id.clone();
                            let can_dispute = matches!(r.status.as_str(), "pending" | "confirmed");
                            let summary = format!("{} — {}", r.grader, r.cert_number);
                            let status_badge = status_badge_view(&r.status);
                            let evidence = r.evidence.clone();
                            let created_at = r.created_at.clone();
                            let non_disputable_msg = match r.status.as_str() {
                                "rejected" => "This report was rejected — no action is needed.",
                                "disputed" => "A dispute has already been filed on this report.",
                                _ => "This report cannot be disputed in its current state.",
                            };

                            view! {
                                // Report summary card
                                <div class="border border-gray-200 rounded-lg p-4 bg-gray-50 mb-6">
                                    <div class="flex items-center justify-between mb-2">
                                        <span class="font-mono font-medium text-gray-900">
                                            {summary}
                                        </span>
                                        {status_badge}
                                    </div>
                                    {evidence.map(|e| view! {
                                        <p class="text-sm text-gray-600 mt-1">"Evidence: " {e}</p>
                                    })}
                                    <p class="text-xs text-gray-400 mt-2">"Reported: " {created_at}</p>
                                </div>

                                // Success state
                                {move || submitted.get().then(|| view! {
                                    <div class="rounded-md bg-green-50 border border-green-200 p-4 mb-4">
                                        <p class="text-green-800 font-medium">"Dispute submitted"</p>
                                        <p class="text-green-700 text-sm mt-1">
                                            "A moderator will review your dispute and the original report. \
                                             The report is now marked as disputed — no automatic action will be taken."
                                        </p>
                                    </div>
                                })}

                                {move || error.get().map(|e| view! {
                                    <div class="rounded-md bg-red-50 border border-red-200 p-4 mb-4">
                                        <p class="text-red-800 font-medium">"Submission failed"</p>
                                        <p class="text-red-700 text-sm mt-1">{e}</p>
                                    </div>
                                })}

                                // Dispute form
                                {(!submitted.get() && can_dispute).then(move || {
                                    let rid = report_id.clone();
                                    let handle_submit = move |ev: leptos::ev::SubmitEvent| {
                                        ev.prevent_default();
                                        let r = reason.get().trim().to_string();
                                        if r.is_empty() {
                                            set_error.set(Some("Reason is required.".into()));
                                            return;
                                        }
                                        set_busy.set(true);
                                        set_error.set(None);
                                        let id = rid.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            match api::submit_dispute(id, r).await {
                                                Ok(()) => set_submitted.set(true),
                                                Err(e) => set_error.set(Some(e)),
                                            }
                                            set_busy.set(false);
                                        });
                                    };

                                    view! {
                                        <form on:submit=handle_submit>
                                            <div class="mb-4">
                                                <label for="reason" class="block text-sm font-medium text-gray-700 mb-1">
                                                    "Your counter-claim" <span class="text-red-500">"*"</span>
                                                </label>
                                                <textarea
                                                    id="reason"
                                                    rows="4"
                                                    required
                                                    placeholder="Explain why you believe this report is inaccurate — include proof of legitimate ownership, purchase receipts, or other relevant details."
                                                    class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                                                    prop:value=move || reason.get()
                                                    on:input=move |e| reason.set(event_target_value(&e))
                                                />
                                                <p class="mt-1 text-xs text-gray-500">
                                                    "Your dispute notes are shared only with platform moderators, not publicly."
                                                </p>
                                            </div>

                                            <div class="rounded-md bg-blue-50 border border-blue-200 p-3 mb-4 text-sm text-blue-800">
                                                "Submitting a dispute does not remove the report — a moderator reviews both sides and makes the final decision."
                                            </div>

                                            <div class="flex gap-3 items-center">
                                                <button
                                                    type="submit"
                                                    disabled=move || busy.get()
                                                    class="bg-orange-600 hover:bg-orange-700 disabled:bg-orange-400 text-white font-medium px-4 py-2 rounded-md text-sm transition-colors"
                                                >
                                                    {move || if busy.get() { "Submitting…" } else { "Submit dispute" }}
                                                </button>
                                                <A href="/stolen/my-reports" attr:class="text-sm text-gray-600 hover:underline">
                                                    "Cancel"
                                                </A>
                                            </div>
                                        </form>
                                    }
                                })}

                                // Non-disputable state
                                {(!can_dispute && !submitted.get()).then(|| view! {
                                    <div class="rounded-md bg-gray-50 border border-gray-200 p-4">
                                        <p class="text-gray-700">
                                            {non_disputable_msg}
                                        </p>
                                        <A href="/stolen/my-reports" attr:class="mt-2 inline-block text-sm text-gray-600 underline">
                                            "← Back to my reports"
                                        </A>
                                    </div>
                                })}
                            }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

fn status_badge_view(status: &str) -> impl IntoView {
    let (class, text) = match status {
        "pending" => ("bg-amber-100 text-amber-800", "Pending"),
        "confirmed" => ("bg-green-100 text-green-800", "Confirmed"),
        "disputed" => ("bg-orange-100 text-orange-800", "Disputed"),
        "rejected" => ("bg-red-100 text-red-800", "Rejected"),
        _ => ("bg-gray-100 text-gray-700", "Unknown"),
    };
    view! {
        <span class=format!("inline-flex items-center px-2 py-0.5 rounded text-xs font-medium {class}")>
            {text}
        </span>
    }
}
