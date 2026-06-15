//! Platform-moderator moderation queue (CSR).

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::api;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueReport {
    pub id: String,
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub evidence: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[component]
pub fn ModeratorQueue() -> impl IntoView {
    let (reload, set_reload) = signal(0u32);

    let queue = LocalResource::new(move || {
        let _ = reload.get();
        async move { api::fetch_moderator_queue().await }
    });

    let trigger_reload = move || set_reload.update(|n| *n += 1);

    view! {
        <div class="max-w-4xl mx-auto p-6">
            <h1 class="text-2xl font-bold text-gray-900 mb-2">"Moderation Queue"</h1>
            <p class="text-sm text-gray-600 mb-6">
                "Review pending stolen-card reports. Confirmed reports are added to the community \
                 stolen-cert list; rejected reports are removed with an audit record."
            </p>

            <Suspense fallback=move || view! { <QueueLoading /> }>
                {move || {
                    queue.get().as_deref().map(|result| match result {
                        Err(e) if e.contains("403") || e.contains("moderator") => {
                            view! { <AccessDenied /> }.into_any()
                        }
                        Err(e) => {
                            let e = e.clone();
                            view! {
                                <div class="rounded-md bg-red-50 border border-red-200 p-4">
                                    <p class="text-red-800">"Failed to load queue: " {e}</p>
                                </div>
                            }.into_any()
                        }
                        Ok(items) if items.is_empty() => view! { <QueueEmpty /> }.into_any(),
                        Ok(items) => {
                            let items = items.clone();
                            view! {
                                <div class="space-y-4">
                                    <p class="text-sm text-gray-500">
                                        {items.len()} " report(s) pending review"
                                    </p>
                                    <For
                                        each=move || items.clone()
                                        key=|r| r.id.clone()
                                        children=move |report| {
                                            view! {
                                                <QueueItem
                                                    report=report
                                                    on_action=trigger_reload
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
    let expanded = RwSignal::new(false);
    let moderator_notes = RwSignal::new(String::new());

    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let report_id = report.id.clone();
    let on_action_clone = on_action.clone();

    let handle_confirm = {
        let report_id = report_id.clone();
        move |_: leptos::ev::MouseEvent| {
            set_busy.set(true);
            set_error.set(None);
            let id = report_id.clone();
            let notes = moderator_notes.get();
            let on_action = on_action_clone.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api::confirm_stolen_report(id, (!notes.is_empty()).then_some(notes)).await {
                    Ok(()) => on_action(),
                    Err(e) => set_error.set(Some(format!("Confirm failed: {e}"))),
                }
                set_busy.set(false);
            });
        }
    };

    let handle_reject = {
        let report_id = report_id.clone();
        move |_: leptos::ev::MouseEvent| {
            set_busy.set(true);
            set_error.set(None);
            let id = report_id.clone();
            let notes = moderator_notes.get();
            let on_action = on_action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api::reject_stolen_report(id, (!notes.is_empty()).then_some(notes)).await {
                    Ok(()) => on_action(),
                    Err(e) => set_error.set(Some(format!("Reject failed: {e}"))),
                }
                set_busy.set(false);
            });
        }
    };

    view! {
        <div class="border border-gray-200 rounded-lg bg-white shadow-sm overflow-hidden">
            <div class="p-4 flex items-start justify-between gap-4">
                <div class="flex-1">
                    <div class="flex items-center gap-3 mb-1">
                        <span class="font-mono font-medium text-gray-900">
                            {format!("{} — {}", report.grader, report.cert_number)}
                        </span>
                        {(report.status == "disputed").then(|| view! {
                            <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-orange-100 text-orange-800">
                                "Disputed"
                            </span>
                        })}
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

                    {move || error.get().map(|e| view! {
                        <p class="text-red-700 text-sm">{e}</p>
                    })}

                    <div class="flex gap-3">
                        <button
                            disabled=move || busy.get()
                            on:click=handle_confirm.clone()
                            class="bg-green-600 hover:bg-green-700 disabled:bg-green-400 text-white text-sm font-medium px-4 py-2 rounded-md transition-colors"
                        >
                            {move || if busy.get() { "Working…" } else { "✓ Confirm" }}
                        </button>
                        <button
                            disabled=move || busy.get()
                            on:click=handle_reject.clone()
                            class="bg-red-600 hover:bg-red-700 disabled:bg-red-400 text-white text-sm font-medium px-4 py-2 rounded-md transition-colors"
                        >
                            {move || if busy.get() { "Working…" } else { "✗ Reject" }}
                        </button>
                    </div>
                </div>
            })}
        </div>
    }
}

#[component]
fn QueueEmpty() -> impl IntoView {
    view! {
        <div class="text-center py-16 text-gray-500">
            <p class="text-lg font-medium text-gray-900 mb-1">"Queue is clear"</p>
            <p class="text-sm">"No pending reports — all caught up."</p>
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
            }).collect_view()}
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
