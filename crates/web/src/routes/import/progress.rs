//! Job progress and result display components.

use leptos::prelude::*;
use uuid::Uuid;

use crate::routes::import::api::{ImportStatus, import_error_report_url};

/// Shows live import progress, polling every 2 s until done/failed.
#[component]
pub fn ImportProgressView(
    job_id: Uuid,
    on_done: Callback<ImportStatus>,
) -> impl IntoView {
    let status = RwSignal::<Option<ImportStatus>>::new(None);
    let fetch_error = RwSignal::<Option<String>>::new(None);

    // CSR-only: kick off polling loop
    #[cfg(feature = "csr")]
    {
        use crate::routes::import::api::get_import_status;

        leptos::task::spawn_local(async move {
            loop {
                match get_import_status(job_id).await {
                    Ok(s) => {
                        let done = s.status == "done" || s.status == "failed";
                        status.set(Some(s.clone()));
                        if done {
                            on_done.run(s);
                            break;
                        }
                    }
                    Err(e) => {
                        fetch_error.set(Some(e));
                        // keep polling so transient errors don't lock the UI
                    }
                }
                gloo_timers::future::TimeoutFuture::new(2000).await;
            }
        });
    }

    view! {
        <div class="space-y-4">
            <h2 class="text-lg font-semibold">"Importing…"</h2>

            {move || fetch_error.get().map(|e| view! {
                <p class="text-sm text-red-600">"Error polling status: " {e}</p>
            })}

            {move || {
                match status.get() {
                    None => view! {
                        <p class="text-sm text-gray-500 animate-pulse">"Starting import…"</p>
                    }.into_any(),
                    Some(s) => {
                        let pct: u32 = s.total_rows
                            .filter(|&t| t > 0)
                            .map(|t| ((s.processed_rows * 100) / t) as u32)
                            .unwrap_or(0);

                        view! {
                            <div class="space-y-2">
                                <div class="flex justify-between text-sm text-gray-600">
                                    <span>
                                        "Processed: "
                                        {s.processed_rows}
                                        " / "
                                        {s.total_rows.unwrap_or(0)}
                                    </span>
                                    <span>{pct} "%"</span>
                                </div>
                                <div class="w-full bg-gray-200 rounded-full h-2">
                                    <div
                                        class="bg-blue-600 h-2 rounded-full transition-all duration-500"
                                        style=format!("width: {}%", pct)
                                    />
                                </div>
                                <p class="text-xs text-gray-500">
                                    "Imported: " {s.imported_rows}
                                    " · Skipped: " {s.skipped_rows}
                                </p>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Shows the final import result with an optional error-report download link.
#[component]
pub fn ImportResultView(
    status: ImportStatus,
    on_import_again: Callback<()>,
) -> impl IntoView {
    let success = status.status == "done";
    let job_id = status.job_id;
    let has_errors = status.has_error_report;
    let skipped = status.skipped_rows;
    let imported = status.imported_rows;
    let err_msg = status.error_message.clone();

    view! {
        <div class="space-y-4">
            // Header
            {if success {
                view! {
                    <div class="flex items-center gap-2">
                        <span class="text-green-600 text-2xl">"✓"</span>
                        <h2 class="text-lg font-semibold text-green-700">"Import complete"</h2>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="flex items-center gap-2">
                        <span class="text-red-600 text-2xl">"✗"</span>
                        <h2 class="text-lg font-semibold text-red-700">"Import failed"</h2>
                    </div>
                }.into_any()
            }}

            // Stats box
            <div class="bg-gray-50 rounded-lg p-4 text-sm space-y-1.5 border">
                <p>"Rows imported: " <strong>{imported}</strong></p>
                <p>"Rows skipped: " <strong>{skipped}</strong></p>
                {err_msg.map(|e| view! {
                    <p class="text-red-600">"Error: " {e}</p>
                })}
            </div>

            // Error-report download
            {if has_errors {
                view! {
                    <a
                        href=import_error_report_url(job_id)
                        download=format!("import-errors-{job_id}.csv")
                        class="inline-flex items-center gap-1 px-4 py-2 text-sm border border-yellow-400 text-yellow-800 bg-yellow-50 rounded hover:bg-yellow-100"
                    >
                        "⬇ Download skipped-row report (" {skipped} " rows)"
                    </a>
                }.into_any()
            } else {
                view! { <span /> }.into_any()
            }}

            <button
                type="button"
                class="px-4 py-2 text-sm border rounded hover:bg-gray-50"
                on:click=move |_| on_import_again.run(())
            >
                "Import another file"
            </button>
        </div>
    }
}
