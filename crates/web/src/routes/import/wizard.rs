//! Import wizard — 4-step flow:
//!   1. Upload  → choose / drag-and-drop the CSV file
//!   2. Map     → assign CSV headers to catalogue fields
//!   3. Preview → inspect sample rows + confirm
//!   4. Progress → live progress bar → result report

use std::collections::HashMap;

use leptos::prelude::*;
use uuid::Uuid;

use crate::routes::import::{
    api::{ColumnMap, DetectedColumns, ImportStatus, StartImportResponse, import_template_url},
    column_map::{ColumnMappingStep, PreviewTable},
    progress::{ImportProgressView, ImportResultView},
};

// ---------------------------------------------------------------------------
// Wizard state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum WizardStep {
    Upload,
    Map(StartImportResponse),
    Preview {
        job_id: Uuid,
        headers: Vec<String>,
        column_map: ColumnMap,
        preview_rows: Vec<HashMap<String, String>>,
    },
    InProgress { job_id: Uuid },
    Done(ImportStatus),
    Error(String),
}

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

/// Route: /import
#[component]
pub fn ImportWizardPage() -> impl IntoView {
    let step = RwSignal::new(WizardStep::Upload);

    view! {
        <div class="max-w-2xl mx-auto px-4 py-8 space-y-6">
            <WizardStepper step=step.read_only() />

            {move || matches!(step.get(), WizardStep::Upload).then(|| view! {
                <div class="text-sm text-gray-600">
                    "Don't have a CSV yet? "
                    <a href=import_template_url() download="cardguard-template.csv"
                       class="text-blue-600 underline">"Download the template"</a>
                    " and fill it in."
                </div>
            })}

            {move || match step.get() {
                WizardStep::Upload => view! {
                    <UploadStep on_uploaded=Callback::new(move |resp| {
                        step.set(WizardStep::Map(resp));
                    }) />
                }.into_any(),

                WizardStep::Map(resp) => {
                    let headers = resp.detected_columns.headers.clone();
                    let initial_map = resp.detected_columns.mapping.clone();
                    let warnings = resp.detected_columns.warnings.clone();
                    let preview_rows = resp.preview_rows.clone();
                    let job_id = resp.job_id;
                    let headers_for_preview = headers.clone();

                    view! {
                        <div class="space-y-6">
                            {(!warnings.is_empty()).then(|| view! {
                                <div class="bg-yellow-50 border border-yellow-300 rounded p-3 text-sm text-yellow-800">
                                    <p class="font-medium mb-1">"Column mapping incomplete:"</p>
                                    <ul class="list-disc list-inside space-y-0.5">
                                        {warnings.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view()}
                                    </ul>
                                </div>
                            })}

                            <ColumnMappingStep
                                headers=headers.clone()
                                initial_map=initial_map
                                on_confirm=Callback::new(move |confirmed_map: ColumnMap| {
                                    step.set(WizardStep::Preview {
                                        job_id,
                                        headers: headers_for_preview.clone(),
                                        column_map: confirmed_map,
                                        preview_rows: preview_rows.clone(),
                                    });
                                })
                                on_back=Callback::new(move |_| step.set(WizardStep::Upload))
                            />
                        </div>
                    }.into_any()
                },

                WizardStep::Preview { job_id, headers, column_map, preview_rows } => {
                    let headers_back = headers.clone();
                    let map_back = column_map.clone();

                    view! {
                        <div class="space-y-6">
                            <div>
                                <h2 class="text-lg font-semibold mb-2">"Preview (first 5 rows)"</h2>
                                <PreviewTable headers=headers.clone() rows=preview_rows.clone() />
                            </div>
                            <div class="flex gap-3">
                                <button
                                    type="button"
                                    class="px-4 py-2 text-sm border rounded hover:bg-gray-50"
                                    on:click=move |_| {
                                        step.set(WizardStep::Map(StartImportResponse {
                                            job_id,
                                            status: "queued".to_owned(),
                                            detected_columns: DetectedColumns {
                                                headers: headers_back.clone(),
                                                mapping: map_back.clone(),
                                                warnings: vec![],
                                            },
                                            preview_rows: vec![],
                                        }));
                                    }
                                >
                                    "Back"
                                </button>
                                <CommitButton
                                    job_id=job_id
                                    column_map=column_map.clone()
                                    on_started=Callback::new(move |_| {
                                        step.set(WizardStep::InProgress { job_id });
                                    })
                                    on_error=Callback::new(move |e| {
                                        step.set(WizardStep::Error(e));
                                    })
                                />
                            </div>
                        </div>
                    }.into_any()
                },

                WizardStep::InProgress { job_id } => view! {
                    <ImportProgressView
                        job_id=job_id
                        on_done=Callback::new(move |s: ImportStatus| {
                            step.set(WizardStep::Done(s));
                        })
                    />
                }.into_any(),

                WizardStep::Done(status) => view! {
                    <ImportResultView
                        status=status
                        on_import_again=Callback::new(move |_| {
                            step.set(WizardStep::Upload);
                        })
                    />
                }.into_any(),

                WizardStep::Error(msg) => view! {
                    <div class="space-y-4">
                        <div class="bg-red-50 border border-red-200 rounded p-4 text-sm text-red-800">
                            <p class="font-medium mb-1">"Upload failed"</p>
                            <p>{msg}</p>
                        </div>
                        <button
                            type="button"
                            class="px-4 py-2 text-sm border rounded hover:bg-gray-50"
                            on:click=move |_| step.set(WizardStep::Upload)
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
// Step 1: Upload
// ---------------------------------------------------------------------------

#[component]
fn UploadStep(on_uploaded: Callback<StartImportResponse>) -> impl IntoView {
    let file_name = RwSignal::<Option<String>>::new(None);
    let is_uploading = RwSignal::new(false);
    let error = RwSignal::<Option<String>>::new(None);

    let handle_file_change = move |ev: leptos::ev::Event| {
        #[cfg(feature = "csr")]
        {
            use wasm_bindgen::JsCast as _;
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        file_name.set(Some(file.name()));
                    }
                }
            }
        }
        let _ = ev;
    };

    let upload = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();

        #[cfg(feature = "csr")]
        {
            use wasm_bindgen::JsCast as _;

            let form = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlFormElement>().ok());

            let form = match form {
                Some(f) => f,
                None => {
                    error.set(Some("Could not access form".to_owned()));
                    return;
                }
            };

            let fd = match web_sys::FormData::new_with_form(&form) {
                Ok(fd) => fd,
                Err(_) => {
                    error.set(Some("Could not build form data".to_owned()));
                    return;
                }
            };

            // Workspace id — in the full app this comes from auth context.
            let _ = fd.append_with_str("workspace_id", &Uuid::nil().to_string());

            is_uploading.set(true);
            error.set(None);

            leptos::task::spawn_local(async move {
                let request = match gloo_net::http::Request::post("/api/catalogue/import")
                    .body(fd)
                {
                    Ok(r) => r,
                    Err(e) => {
                        is_uploading.set(false);
                        error.set(Some(format!("Failed to build request: {e}")));
                        return;
                    }
                };
                let resp = request.send().await;

                is_uploading.set(false);

                match resp {
                    Err(e) => error.set(Some(format!("Network error: {e}"))),
                    Ok(r) => {
                        if r.ok() {
                            match r.json::<StartImportResponse>().await {
                                Ok(data) => on_uploaded.run(data),
                                Err(e) => error.set(Some(format!("Parse error: {e}"))),
                            }
                        } else {
                            let status = r.status();
                            let body = r.text().await.unwrap_or_default();
                            error.set(Some(format!("Server error {status}: {body}")));
                        }
                    }
                }
            });
        }
    };

    view! {
        <div class="space-y-6">
            <div>
                <h2 class="text-xl font-semibold mb-1">"Import cards from CSV"</h2>
                <p class="text-sm text-gray-600">
                    "Upload a CSV file with your collection. "
                    "Supported: UTF-8 and UTF-8 with BOM. Max 50 MB."
                </p>
            </div>

            <form on:submit=upload enctype="multipart/form-data" class="space-y-4">
                <label
                    for="csv-file"
                    class="flex flex-col items-center justify-center w-full h-40 border-2 border-dashed border-gray-300 rounded-lg cursor-pointer hover:bg-gray-50 transition"
                >
                    {move || {
                        if let Some(name) = file_name.get() {
                            view! {
                                <div class="text-center">
                                    <p class="text-2xl mb-1">"📄"</p>
                                    <p class="text-sm font-medium text-gray-700">{name}</p>
                                    <p class="text-xs text-gray-500">"Click to change"</p>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="text-center text-gray-500">
                                    <p class="text-3xl mb-2">"⬆"</p>
                                    <p class="text-sm font-medium">"Click to upload a CSV"</p>
                                    <p class="text-xs">"or drag and drop"</p>
                                </div>
                            }.into_any()
                        }
                    }}
                    <input
                        id="csv-file"
                        type="file"
                        name="file"
                        accept=".csv,text/csv"
                        class="sr-only"
                        on:change=handle_file_change
                    />
                </label>

                {move || error.get().map(|e| view! {
                    <div class="bg-red-50 border border-red-200 rounded p-3 text-sm text-red-700">{e}</div>
                })}

                <button
                    type="submit"
                    class="w-full py-2.5 px-4 bg-blue-600 text-white text-sm font-medium rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
                    disabled=move || is_uploading.get() || file_name.get().is_none()
                >
                    {move || if is_uploading.get() { "Uploading…" } else { "Upload & analyse" }}
                </button>
            </form>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Commit button
// ---------------------------------------------------------------------------

#[component]
fn CommitButton(
    job_id: Uuid,
    column_map: ColumnMap,
    on_started: Callback<()>,
    on_error: Callback<String>,
) -> impl IntoView {
    let is_loading = RwSignal::new(false);

    let commit = move |_: leptos::ev::MouseEvent| {
        #[cfg(feature = "csr")]
        {
            is_loading.set(true);
            let map = column_map.clone();

            leptos::task::spawn_local(async move {
                let body = serde_json::json!({ "column_map": serde_json::to_value(&map).unwrap_or_default() });

                let resp = gloo_net::http::Request::post(&format!(
                    "/api/catalogue/import/{job_id}/confirm"
                ))
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
                    Ok(_) => on_started.run(()),
                }
            });
        }
    };

    view! {
        <button
            type="button"
            class="px-5 py-2 text-sm bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50"
            disabled=move || is_loading.get()
            on:click=commit
        >
            {move || if is_loading.get() { "Starting import…" } else { "Confirm & import" }}
        </button>
    }
}

// ---------------------------------------------------------------------------
// Stepper breadcrumb
// ---------------------------------------------------------------------------

#[component]
fn WizardStepper(step: ReadSignal<WizardStep>) -> impl IntoView {
    const STEPS: &[&str] = &["Upload", "Map columns", "Preview", "Importing", "Done"];

    let active = move || match step.get() {
        WizardStep::Upload => 0usize,
        WizardStep::Map(_) => 1,
        WizardStep::Preview { .. } => 2,
        WizardStep::InProgress { .. } => 3,
        WizardStep::Done(_) | WizardStep::Error(_) => 4,
    };

    view! {
        <nav aria-label="Import steps" class="flex items-center gap-1 flex-wrap">
            {STEPS.iter().enumerate().map(|(i, label)| {
                let label = *label;
                view! {
                    <>
                        {(i > 0).then(|| view! { <span class="text-gray-300 mx-1">"›"</span> })}
                        <span class=move || match i.cmp(&active()) {
                            std::cmp::Ordering::Equal => "text-sm font-semibold text-blue-600",
                            std::cmp::Ordering::Less => "text-sm text-gray-400 line-through",
                            std::cmp::Ordering::Greater => "text-sm text-gray-400",
                        }>
                            {label}
                        </span>
                    </>
                }
            }).collect_view()}
        </nav>
    }
}
