use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;

use super::api::{ExportFilters, ExportStatusResponse, StartExportRequest, StartExportResponse};

#[derive(Clone, PartialEq)]
enum ExportStep {
    Form,
    Waiting,
    Ready,
    Failed,
}

/// CSV export page — filter form, progress, and download link.
#[component]
pub fn ExportPage(workspace_id: Uuid) -> impl IntoView {
    let (step, set_step) = signal(ExportStep::Form);
    let (job_id, set_job_id) = signal(Option::<Uuid>::None);
    let (row_count, set_row_count) = signal(Option::<i64>::None);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    // filter controls
    let (set_id_filter, set_set_id_filter) = signal(String::new());
    let (in_stock_only, set_in_stock_only) = signal(false);
    let (include_raw, set_include_raw) = signal(true);
    let (include_graded, set_include_graded) = signal(true);

    let handle_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let filters = ExportFilters {
            set_id: {
                let v = set_id_filter.get();
                if v.is_empty() { None } else { Some(v) }
            },
            in_stock_only: Some(in_stock_only.get()),
            include_raw: Some(include_raw.get()),
            include_graded: Some(include_graded.get()),
            condition: None,
        };
        let req = StartExportRequest { workspace_id, filters };

        set_error_msg.set(None);
        set_step.set(ExportStep::Waiting);

        spawn_local(async move {
            let body = match serde_json::to_string(&req) {
                Ok(b) => b,
                Err(e) => {
                    set_step.set(ExportStep::Failed);
                    set_error_msg.set(Some(e.to_string()));
                    return;
                }
            };
            let resp = gloo_net::http::Request::post("/api/catalogue/export")
                .header("Content-Type", "application/json")
                .body(body)
                .ok()
                .map(|r| async move { r.send().await });

            let Some(resp_fut) = resp else {
                set_step.set(ExportStep::Failed);
                set_error_msg.set(Some("Failed to build request".into()));
                return;
            };
            let resp = match resp_fut.await {
                Ok(r) if r.ok() => r,
                Ok(r) => {
                    set_step.set(ExportStep::Failed);
                    set_error_msg.set(Some(format!("Server error: HTTP {}", r.status())));
                    return;
                }
                Err(e) => {
                    set_step.set(ExportStep::Failed);
                    set_error_msg.set(Some(e.to_string()));
                    return;
                }
            };
            let start: StartExportResponse = match resp.json().await {
                Ok(r) => r,
                Err(e) => {
                    set_step.set(ExportStep::Failed);
                    set_error_msg.set(Some(format!("Invalid response: {e}")));
                    return;
                }
            };
            let jid = start.job_id;
            set_job_id.set(Some(jid));

            // poll until done
            loop {
                gloo_timers::future::TimeoutFuture::new(2_000).await;
                let url = format!("/api/catalogue/export/{jid}");
                match gloo_net::http::Request::get(&url).send().await {
                    Err(e) => {
                        set_step.set(ExportStep::Failed);
                        set_error_msg.set(Some(e.to_string()));
                        break;
                    }
                    Ok(r) if !r.ok() => {
                        set_step.set(ExportStep::Failed);
                        set_error_msg.set(Some(format!("HTTP {}", r.status())));
                        break;
                    }
                    Ok(r) => match r.json::<ExportStatusResponse>().await {
                        Err(e) => {
                            set_step.set(ExportStep::Failed);
                            set_error_msg.set(Some(e.to_string()));
                            break;
                        }
                        Ok(s) => {
                            if s.status == "done" && s.download_ready {
                                set_row_count.set(s.row_count);
                                set_step.set(ExportStep::Ready);
                                break;
                            } else if s.status == "failed" {
                                set_step.set(ExportStep::Failed);
                                set_error_msg.set(s.error_message);
                                break;
                            }
                            // still running — loop
                        }
                    },
                }
            }
        });
    };

    let handle_reset = move |_| {
        set_step.set(ExportStep::Form);
        set_job_id.set(None);
        set_row_count.set(None);
        set_error_msg.set(None);
    };

    view! {
        <div class="export-page">
            <h2>"Export Catalogue"</h2>

            {move || match step.get() {
                ExportStep::Form => view! {
                    <form on:submit=handle_submit>
                        <div>
                            <label>"Set ID filter (optional):"</label>
                            <input
                                type="text"
                                placeholder="e.g. sv1"
                                on:input=move |ev| set_set_id_filter.set(event_target_value(&ev))
                                prop:value=set_id_filter
                            />
                        </div>
                        <div>
                            <label>
                                <input
                                    type="checkbox"
                                    prop:checked=in_stock_only
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        let checked = ev.target()
                                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                            .map(|i| i.checked())
                                            .unwrap_or(false);
                                        set_in_stock_only.set(checked);
                                    }
                                />
                                " In stock only"
                            </label>
                        </div>
                        <div>
                            <label>
                                <input
                                    type="checkbox"
                                    prop:checked=include_raw
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        let checked = ev.target()
                                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                            .map(|i| i.checked())
                                            .unwrap_or(true);
                                        set_include_raw.set(checked);
                                    }
                                />
                                " Include raw cards"
                            </label>
                        </div>
                        <div>
                            <label>
                                <input
                                    type="checkbox"
                                    prop:checked=include_graded
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        let checked = ev.target()
                                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                                            .map(|i| i.checked())
                                            .unwrap_or(true);
                                        set_include_graded.set(checked);
                                    }
                                />
                                " Include graded cards"
                            </label>
                        </div>
                        <button type="submit">"Export"</button>
                    </form>
                }.into_any(),

                ExportStep::Waiting => view! {
                    <div>
                        <p>"Building your export…"</p>
                    </div>
                }.into_any(),

                ExportStep::Ready => view! {
                    <div>
                        <p class="success">
                            "Export ready — "
                            {row_count.get().map(|n| format!("{n} rows"))}
                        </p>
                        {move || job_id.get().map(|jid| view! {
                            <a
                                href=format!("/api/catalogue/export/{jid}/download")
                                download
                                class="download-link"
                            >
                                "Download CSV"
                            </a>
                        })}
                        <button on:click=handle_reset style="margin-left:1rem;">
                            "New export"
                        </button>
                    </div>
                }.into_any(),

                ExportStep::Failed => view! {
                    <div>
                        <p class="error">
                            "Export failed: "
                            {error_msg.get().unwrap_or_else(|| "unknown error".into())}
                        </p>
                        <button on:click=handle_reset>"Try again"</button>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}
