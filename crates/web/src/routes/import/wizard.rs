use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;
use wasm_bindgen::JsCast;

use super::{
    api::{ImportStatusResponse, StartImportResponse},
    column_map::ColumnMapTable,
    progress::ImportProgress,
};

#[derive(Clone, PartialEq)]
enum Step {
    Upload,
    Processing,
    Review,
    Done,
}

/// Multi-step CSV import wizard.
///
/// # Props
/// - `workspace_id`: the workspace to import cards into.
#[component]
pub fn ImportWizard(workspace_id: Uuid) -> impl IntoView {
    let (step, set_step) = signal(Step::Upload);
    let (job_id, set_job_id) = signal(Option::<Uuid>::None);
    let (final_status, set_final_status) = signal(Option::<ImportStatusResponse>::None);
    let (upload_error, set_upload_error) = signal(Option::<String>::None);
    let (is_uploading, set_is_uploading) = signal(false);

    // --- upload handler ---
    let handle_upload = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let target: web_sys::HtmlFormElement = ev.target().unwrap().unchecked_into();
        let elements = target.elements();
        let file_input = elements
            .named_item("csv_file")
            .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok());

        let Some(input) = file_input else {
            set_upload_error.set(Some("File input not found.".into()));
            return;
        };
        let Some(files) = input.files() else {
            set_upload_error.set(Some("Could not read file list.".into()));
            return;
        };
        let Some(file) = files.get(0) else {
            set_upload_error.set(Some("Please select a CSV file.".into()));
            return;
        };

        let fd = web_sys::FormData::new().unwrap();
        fd.append_with_blob_and_filename("file", file.as_ref(), &file.name())
            .unwrap();
        fd.append_with_str("workspace_id", &workspace_id.to_string())
            .unwrap();

        set_upload_error.set(None);
        set_is_uploading.set(true);

        spawn_local(async move {
            let request = match gloo_net::http::Request::post("/api/catalogue/import")
                .body(fd)
            {
                Ok(r) => r,
                Err(e) => {
                    set_is_uploading.set(false);
                    set_upload_error.set(Some(format!("Failed to build request: {e}")));
                    return;
                }
            };
            match request.send().await {
                Err(e) => {
                    set_is_uploading.set(false);
                    set_upload_error.set(Some(e.to_string()));
                }
                Ok(resp) if !resp.ok() => {
                    set_is_uploading.set(false);
                    set_upload_error.set(Some(format!("Server error: HTTP {}", resp.status())));
                }
                Ok(resp) => match resp.json::<StartImportResponse>().await {
                    Err(e) => {
                        set_is_uploading.set(false);
                        set_upload_error.set(Some(format!("Invalid response: {e}")));
                    }
                    Ok(r) => {
                        set_is_uploading.set(false);
                        set_job_id.set(Some(r.job_id));
                        set_step.set(Step::Processing);
                    }
                },
            }
        });
    };

    // --- confirm handler ---
    let handle_confirm = move |_| {
        let Some(jid) = job_id.get() else { return };
        spawn_local(async move {
            let url = format!("/api/catalogue/import/{jid}/confirm");
            if let Ok(resp) = gloo_net::http::Request::post(&url).send().await {
                if resp.ok() {
                    set_step.set(Step::Done);
                }
            }
        });
    };

    // --- cancel handler ---
    let handle_cancel = move |_| {
        set_step.set(Step::Upload);
        set_job_id.set(None);
        set_final_status.set(None);
        set_upload_error.set(None);
    };

    // --- progress done callback ---
    let on_progress_done = Callback::new(move |s: ImportStatusResponse| {
        set_final_status.set(Some(s));
        set_step.set(Step::Review);
    });

    view! {
        <div class="import-wizard">
            <h2>"Import CSV"</h2>

            {move || match step.get() {
                // ── Step 1: Upload ──────────────────────────────────────────
                Step::Upload => view! {
                    <div>
                        <p>
                            <a href="/api/catalogue/import/template" download>
                                "Download CSV template"
                            </a>
                        </p>
                        <ColumnMapTable />
                        <form on:submit=handle_upload>
                            <div>
                                <label for="csv_file">"Select CSV file:"</label>
                                <input
                                    id="csv_file"
                                    name="csv_file"
                                    type="file"
                                    accept=".csv,text/csv"
                                />
                            </div>
                            <button type="submit" disabled=is_uploading>
                                {move || if is_uploading.get() { "Uploading…" } else { "Upload" }}
                            </button>
                        </form>
                        {move || upload_error.get().map(|e| view! {
                            <p class="error">{e}</p>
                        })}
                    </div>
                }.into_any(),

                // ── Step 2: Processing ──────────────────────────────────────
                Step::Processing => view! {
                    <div>
                        <p>"Processing your CSV…"</p>
                        {move || job_id.get().map(|jid| view! {
                            <ImportProgress job_id=jid on_done=on_progress_done />
                        })}
                    </div>
                }.into_any(),

                // ── Step 3: Review ──────────────────────────────────────────
                Step::Review => view! {
                    <div>
                        {move || final_status.get().map(|s| {
                            let failed = s.status == "failed";
                            view! {
                                <div>
                                    {if failed {
                                        view! {
                                            <p class="error">"Import failed. Check errors below."</p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <p>
                                                "Ready to import: "
                                                <strong>{s.imported_rows}</strong>
                                                " cards ("
                                                {s.skipped_rows}
                                                " skipped, "
                                                {s.error_count}
                                                " errors)."
                                            </p>
                                        }.into_any()
                                    }}
                                    {job_id.get().map(|jid| {
                                        let show_errors = s.error_count > 0;
                                        view! {
                                            <div class="review-actions">
                                                {(!failed).then(|| view! {
                                                    <button on:click=handle_confirm>"Confirm Import"</button>
                                                })}
                                                <button on:click=handle_cancel>"Cancel"</button>
                                                {show_errors.then(|| view! {
                                                    <a
                                                        href=format!("/api/catalogue/import/{jid}/errors")
                                                        download
                                                    >
                                                        "Download error report"
                                                    </a>
                                                })}
                                            </div>
                                        }
                                    })}
                                </div>
                            }
                        })}
                    </div>
                }.into_any(),

                // ── Step 4: Done ────────────────────────────────────────────
                Step::Done => view! {
                    <div>
                        <p class="success">"Import complete!"</p>
                        {move || final_status.get().map(|s| view! {
                            <p>
                                {s.imported_rows} " cards imported successfully."
                            </p>
                        })}
                        <button on:click=handle_cancel>"Import another file"</button>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}
