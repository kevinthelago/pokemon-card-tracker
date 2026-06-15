//! Export page — lets the user export the whole catalogue or a filtered subset.

use leptos::prelude::*;
use uuid::Uuid;

use crate::routes::import::api::{ExportStatus, export_download_url};
#[cfg(feature = "csr")]
use crate::routes::import::api::ExportFilters;

#[derive(Debug, Clone, PartialEq)]
enum ExportStep {
    Configure,
    Exporting { job_id: Uuid },
    Done { job_id: Uuid, row_count: Option<i64> },
    Error(String),
}

/// Route: /export
#[component]
pub fn ExportPage() -> impl IntoView {
    let step = RwSignal::new(ExportStep::Configure);
    // Workspace id from auth context (stub — real app injects via context).
    let workspace_id = Uuid::nil();

    view! {
        <div class="max-w-xl mx-auto px-4 py-8 space-y-6">
            <h1 class="text-2xl font-semibold">"Export catalogue to CSV"</h1>

            {move || match step.get() {
                ExportStep::Configure => view! {
                    <ExportConfigForm
                        workspace_id=workspace_id
                        on_started=Callback::new(move |job_id: Uuid| {
                            step.set(ExportStep::Exporting { job_id });
                        })
                        on_error=Callback::new(move |e| {
                            step.set(ExportStep::Error(e));
                        })
                    />
                }.into_any(),

                ExportStep::Exporting { job_id } => view! {
                    <ExportProgressView
                        job_id=job_id
                        on_done=Callback::new(move |s: ExportStatus| {
                            if s.status == "done" {
                                step.set(ExportStep::Done { job_id, row_count: s.row_count });
                            } else {
                                step.set(ExportStep::Error(
                                    s.error_message.unwrap_or_else(|| "Export failed".to_owned()),
                                ));
                            }
                        })
                    />
                }.into_any(),

                ExportStep::Done { job_id, row_count } => view! {
                    <div class="space-y-4">
                        <div class="flex items-center gap-2">
                            <span class="text-green-600 text-2xl">"✓"</span>
                            <h2 class="text-lg font-semibold text-green-700">"Export ready"</h2>
                        </div>
                        {row_count.map(|r| view! {
                            <p class="text-sm text-gray-600">{r} " rows exported."</p>
                        })}
                        <a
                            href=export_download_url(job_id)
                            download=format!("cardguard-export-{job_id}.csv")
                            class="inline-flex items-center gap-1 px-5 py-2.5 text-sm bg-green-600 text-white rounded hover:bg-green-700"
                        >
                            "⬇ Download CSV"
                        </a>
                        <div>
                            <button
                                type="button"
                                class="text-sm text-blue-600 underline"
                                on:click=move |_| step.set(ExportStep::Configure)
                            >
                                "Export again with different filters"
                            </button>
                        </div>
                    </div>
                }.into_any(),

                ExportStep::Error(msg) => view! {
                    <div class="space-y-4">
                        <div class="bg-red-50 border border-red-200 rounded p-4 text-sm text-red-800">
                            <p class="font-medium mb-1">"Export failed"</p>
                            <p>{msg}</p>
                        </div>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm border rounded hover:bg-gray-50"
                            on:click=move |_| step.set(ExportStep::Configure)
                        >
                            "Try again"
                        </button>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Config form
// ---------------------------------------------------------------------------

#[component]
fn ExportConfigForm(
    workspace_id: Uuid,
    on_started: Callback<Uuid>,
    on_error: Callback<String>,
) -> impl IntoView {
    let set_code = RwSignal::<String>::new(String::new());
    let include_raw = RwSignal::new(true);
    let include_graded = RwSignal::new(true);
    let in_stock_only = RwSignal::new(false);
    let is_loading = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();

        #[cfg(feature = "csr")]
        {
            is_loading.set(true);

            let filters = ExportFilters {
                set_code: {
                    let s = set_code.get();
                    if s.is_empty() { None } else { Some(s) }
                },
                condition: None,
                include_raw: Some(include_raw.get()),
                include_graded: Some(include_graded.get()),
                in_stock_only: Some(in_stock_only.get()),
            };

            leptos::task::spawn_local(async move {
                let body = serde_json::json!({
                    "workspace_id": workspace_id,
                    "filters": serde_json::to_value(filters).unwrap_or_default(),
                });

                let resp = gloo_net::http::Request::post("/api/catalogue/export")
                    .json(&body)
                    .unwrap()
                    .send()
                    .await;

                is_loading.set(false);

                match resp {
                    Err(e) => on_error.run(format!("Network error: {e}")),
                    Ok(r) if !r.ok() => {
                        let text = r.text().await.unwrap_or_default();
                        on_error.run(format!("Server error: {text}"));
                    }
                    Ok(r) => {
                        #[derive(serde::Deserialize)]
                        struct Resp { job_id: Uuid }
                        match r.json::<Resp>().await {
                            Ok(d) => on_started.run(d.job_id),
                            Err(e) => on_error.run(format!("Parse error: {e}")),
                        }
                    }
                }
            });
        }
    };

    view! {
        <form on:submit=submit class="space-y-5">
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">
                    "Filter by set code (optional)"
                </label>
                <input
                    type="text"
                    placeholder="e.g. base1"
                    class="border rounded px-3 py-2 text-sm w-full focus:outline-none focus:ring-2 focus:ring-blue-400"
                    on:input=move |ev| set_code.set(event_target_value(&ev))
                />
            </div>

            <fieldset class="space-y-2">
                <legend class="text-sm font-medium text-gray-700">"Include"</legend>
                <label class="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked=move || include_raw.get()
                           on:change=move |ev| include_raw.set(event_target_checked(&ev)) />
                    "Raw / unsealed inventory"
                </label>
                <label class="flex items-center gap-2 text-sm">
                    <input type="checkbox" checked=move || include_graded.get()
                           on:change=move |ev| include_graded.set(event_target_checked(&ev)) />
                    "Graded card instances"
                </label>
            </fieldset>

            <label class="flex items-center gap-2 text-sm">
                <input type="checkbox" checked=move || in_stock_only.get()
                       on:change=move |ev| in_stock_only.set(event_target_checked(&ev)) />
                "In-stock items only (quantity > 0)"
            </label>

            {move || (!include_raw.get() && !include_graded.get()).then(|| view! {
                <p class="text-sm text-yellow-700 bg-yellow-50 border border-yellow-200 rounded p-2">
                    "Select at least one type to export."
                </p>
            })}

            <button
                type="submit"
                class="w-full py-2.5 px-4 bg-blue-600 text-white text-sm font-medium rounded hover:bg-blue-700 disabled:opacity-50"
                disabled=move || is_loading.get() || (!include_raw.get() && !include_graded.get())
            >
                {move || if is_loading.get() { "Starting…" } else { "Export to CSV" }}
            </button>
        </form>
    }
}

// ---------------------------------------------------------------------------
// Progress view
// ---------------------------------------------------------------------------

#[component]
fn ExportProgressView(job_id: Uuid, on_done: Callback<ExportStatus>) -> impl IntoView {
    let status_msg = RwSignal::<Option<String>>::new(None);
    let fetch_error = RwSignal::<Option<String>>::new(None);

    #[cfg(feature = "csr")]
    {
        use crate::routes::import::api::get_export_status;

        leptos::task::spawn_local(async move {
            loop {
                match get_export_status(job_id).await {
                    Ok(s) => {
                        let done = s.status == "done" || s.status == "failed";
                        status_msg.set(Some(s.status.clone()));
                        if done {
                            on_done.run(s);
                            break;
                        }
                    }
                    Err(e) => fetch_error.set(Some(e)),
                }
                gloo_timers::future::TimeoutFuture::new(2000).await;
            }
        });
    }

    view! {
        <div class="space-y-4">
            <p class="text-sm font-medium animate-pulse">"Generating export…"</p>
            {move || fetch_error.get().map(|e| view! {
                <p class="text-sm text-red-600">"Poll error: " {e}</p>
            })}
            {move || status_msg.get().map(|s| view! {
                <p class="text-xs text-gray-500">"Status: " {s}</p>
            })}
        </div>
    }
}
