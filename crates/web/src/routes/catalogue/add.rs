//! Add-a-card page: scan + search + review/confirm (issue #12, C4).

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::scan::camera::ScanStep;

// ── Shared API types (mirrors the API crate) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrintingView {
    pub id: Uuid,
    pub tcg_api_id: String,
    pub name: String,
    pub set_name: String,
    pub number: String,
    pub language: String,
    pub image_url: Option<String>,
    pub rarity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub items: Vec<PrintingView>,
    pub total_count: u32,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScanResult {
    pub barcode: String,
    /// "graded" | "sealed_product" | "printing" | "unknown"
    pub resolved_as: Option<String>,
    pub printing: Option<PrintingView>,
    pub grader: Option<String>,
    pub cert_number: Option<String>,
    pub grade: Option<String>,
    pub needs_manual_entry: bool,
    pub error: Option<String>,
}

// ── Wizard steps ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Step {
    SelectMode,
    Search,
    Scan,
    Review { printing: PrintingView, scan: Option<ScanResult> },
}

// ── Page root ──────────────────────────────────────────────────────────────

#[component]
pub fn AddCardPage() -> impl IntoView {
    let (step, set_step) = signal(Step::SelectMode);

    let go_search = move |_| set_step.set(Step::Search);
    let go_scan = move |_| set_step.set(Step::Scan);
    let go_home = move |_| {
        let window = web_sys::window().unwrap();
        window.location().set_href("/").ok();
    };

    let on_printing_selected = move |printing: PrintingView| {
        set_step.set(Step::Review { printing, scan: None });
    };

    let on_scan_result = move |result: ScanResult| {
        if let Some(printing) = result.printing.clone() {
            set_step.set(Step::Review {
                printing,
                scan: Some(result),
            });
        } else if result.needs_manual_entry || result.resolved_as.as_deref() != Some("graded") {
            // Unrecognised barcode or UPC — fall back to search.
            set_step.set(Step::Search);
        } else {
            // Graded card without a printing — go to review with a placeholder.
            // The user will search for the card identity in the review step.
            set_step.set(Step::Search);
        }
    };

    let on_saved = move || {
        // Redirect to home after successful save.
        let window = web_sys::window().unwrap();
        window.location().set_href("/").ok();
    };

    view! {
        <div class="add-card-page">
            <header class="page-header">
                <button on:click=go_home class="btn-ghost">"← Back"</button>
                <h1 class="page-title">"Add a Card"</h1>
            </header>

            {move || match step.get() {
                Step::SelectMode => view! {
                    <ModeSelector on_search=go_search on_scan=go_scan />
                }.into_any(),

                Step::Search => view! {
                    <SearchStep on_select=on_printing_selected />
                }.into_any(),

                Step::Scan => view! {
                    <ScanStep on_result=on_scan_result />
                }.into_any(),

                Step::Review { printing, scan } => view! {
                    <ReviewStep
                        printing=printing
                        scan_result=scan
                        on_saved=on_saved
                    />
                }.into_any(),
            }}
        </div>
    }
}

// ── Mode selector ──────────────────────────────────────────────────────────

#[component]
fn ModeSelector<FS, FF>(on_search: FS, on_scan: FF) -> impl IntoView
where
    FS: Fn(web_sys::MouseEvent) + 'static,
    FF: Fn(web_sys::MouseEvent) + 'static,
{
    view! {
        <div class="mode-selector">
            <p class="mode-prompt">"How would you like to add this card?"</p>
            <div class="mode-buttons">
                <button on:click=on_scan class="mode-btn mode-scan">
                    <span class="mode-icon">"📷"</span>
                    <span class="mode-label">"Scan barcode"</span>
                    <span class="mode-hint">"Graded slab or sealed product UPC"</span>
                </button>
                <button on:click=on_search class="mode-btn mode-search">
                    <span class="mode-icon">"🔍"</span>
                    <span class="mode-label">"Search"</span>
                    <span class="mode-hint">"Name, set, or card number"</span>
                </button>
            </div>
        </div>
    }
}

// ── Search step ────────────────────────────────────────────────────────────

#[component]
fn SearchStep<F>(on_select: F) -> impl IntoView
where
    F: Fn(PrintingView) + Clone + Send + Sync + 'static,
{
    let (query, set_query) = signal(String::new());
    let (submitted, set_submitted) = signal(String::new());

    let results = LocalResource::new(move || {
        let q = submitted.get();
        async move {
            if q.is_empty() {
                return Ok::<Option<SearchResults>, String>(None);
            }
            fetch_printings(&q).await.map(Some)
        }
    });

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        set_submitted.set(query.get());
    };

    let on_select_clone = on_select.clone();

    view! {
        <div class="search-step">
            <h2 class="step-title">"Search for a card"</h2>

            <form on:submit=on_submit class="search-form">
                <input
                    type="text"
                    class="search-input"
                    placeholder="Pikachu, swsh1-57, Charizard..."
                    on:input=move |ev| set_query.set(event_target_value(&ev))
                    prop:value=query
                />
                <button type="submit" class="btn-primary">"Search"</button>
            </form>

            <Suspense fallback=|| view! { <p class="loading">"Searching..."</p> }>
                {move || match results.get().as_deref() {
                    None => view! { <div></div> }.into_any(),
                    Some(Ok(None)) => view! { <div></div> }.into_any(),
                    Some(Ok(Some(data))) if data.items.is_empty() => view! {
                        <div class="empty-state">
                            <p>"No cards found. Try a different name or set."</p>
                        </div>
                    }.into_any(),
                    Some(Ok(Some(data))) => {
                        let items = data.items.clone();
                        let on_select2 = on_select_clone.clone();
                        view! {
                            <ul class="printing-list">
                                {items.into_iter().map(|p| {
                                    let p_clone = p.clone();
                                    let on_s = on_select2.clone();
                                    view! {
                                        <li class="printing-item">
                                            <button
                                                class="printing-btn"
                                                on:click=move |_| on_s(p_clone.clone())
                                            >
                                                {p.image_url.as_ref().map(|url| view! {
                                                    <img src=url.clone() alt=p.name.clone() class="card-thumb" />
                                                })}
                                                <div class="printing-info">
                                                    <span class="card-name">{p.name.clone()}</span>
                                                    <span class="card-set">{p.set_name.clone()}</span>
                                                    <span class="card-number">"#"{p.number.clone()}</span>
                                                </div>
                                            </button>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }
                    Some(Err(e)) => {
                        let e = e.clone();
                        view! {
                            <p class="error">"Error: "{e}</p>
                        }.into_any()
                    },
                }}
            </Suspense>
        </div>
    }
}

// ── Review / confirm step ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct SaveRequest {
    workspace_id: String,
    item_kind: String,
    printing_id: Option<String>,
    condition: Option<String>,
    quantity: i32,
    acquisition_cost_cents: Option<i32>,
    grader: Option<String>,
    cert_number: Option<String>,
    grade: Option<String>,
    verification_status: String,
    notes: Option<String>,
    photos: Vec<String>,
}

#[component]
fn ReviewStep<F>(
    printing: PrintingView,
    scan_result: Option<ScanResult>,
    on_saved: F,
) -> impl IntoView
where
    F: Fn() + Clone + 'static,
{
    let is_graded = scan_result
        .as_ref()
        .map(|s| s.resolved_as.as_deref() == Some("graded"))
        .unwrap_or(false);

    let (condition, set_condition) = signal("NM".to_string());
    let (quantity, set_quantity) = signal(1i32);
    let (cost_str, set_cost_str) = signal(String::new());
    let (notes, set_notes) = signal(String::new());
    let (saving, set_saving) = signal(false);
    let (save_error, set_save_error) = signal::<Option<String>>(None);

    let grader = scan_result
        .as_ref()
        .and_then(|s| s.grader.clone())
        .unwrap_or_default();
    let cert_number = scan_result
        .as_ref()
        .and_then(|s| s.cert_number.clone())
        .unwrap_or_default();
    let grade_val = scan_result
        .as_ref()
        .and_then(|s| s.grade.clone());

    let printing_clone = printing.clone();
    let grader_clone = grader.clone();
    let cert_clone = cert_number.clone();
    let grade_clone = grade_val.clone();
    let on_saved_clone = on_saved.clone();

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        set_saving.set(true);
        set_save_error.set(None);

        let cost_cents = cost_str
            .get()
            .parse::<f64>()
            .ok()
            .map(|dollars| (dollars * 100.0) as i32);

        let req = if is_graded {
            SaveRequest {
                // TODO: resolve real workspace_id from auth context.
                workspace_id: "00000000-0000-0000-0000-000000000000".into(),
                item_kind: "graded".into(),
                printing_id: Some(printing_clone.id.to_string()),
                condition: None,
                quantity: 1,
                acquisition_cost_cents: cost_cents,
                grader: Some(grader_clone.clone()),
                cert_number: Some(cert_clone.clone()),
                grade: grade_clone.clone(),
                verification_status: "unverified".into(),
                notes: Some(notes.get()).filter(|s| !s.is_empty()),
                photos: vec![],
            }
        } else {
            SaveRequest {
                workspace_id: "00000000-0000-0000-0000-000000000000".into(),
                item_kind: "raw".into(),
                printing_id: Some(printing_clone.id.to_string()),
                condition: Some(condition.get()),
                quantity: quantity.get(),
                acquisition_cost_cents: cost_cents,
                grader: None,
                cert_number: None,
                grade: None,
                verification_status: "unverified".into(),
                notes: Some(notes.get()).filter(|s| !s.is_empty()),
                photos: vec![],
            }
        };

        let saved_cb = on_saved_clone.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match save_item(req).await {
                Ok(()) => {
                    set_saving.set(false);
                    saved_cb();
                }
                Err(e) => {
                    set_saving.set(false);
                    set_save_error.set(Some(e));
                }
            }
        });
    };

    view! {
        <div class="review-step">
            <h2 class="step-title">"Review & confirm"</h2>

            <div class="card-preview">
                {printing.image_url.as_ref().map(|url| view! {
                    <img src=url.clone() alt=printing.name.clone() class="card-preview-img" />
                })}
                <div class="card-preview-details">
                    <p class="card-name">{printing.name.clone()}</p>
                    <p class="card-set">{printing.set_name.clone()}" · #"{printing.number.clone()}</p>
                    {is_graded.then(|| view! {
                        <div class="graded-info">
                            <span class="grader-badge">{grader.clone()}</span>
                            <span class="cert-label">"Cert: "{cert_number.clone()}</span>
                            {grade_val.as_ref().map(|g| view! {
                                <span class="grade-badge">"Grade "{g.clone()}</span>
                            })}
                            <span class="verification-badge unverified">"Unverified"</span>
                        </div>
                    })}
                </div>
            </div>

            <form on:submit=on_submit class="review-form">
                {(!is_graded).then(|| view! {
                    <div class="form-row">
                        <label class="form-label">"Condition"</label>
                        <select
                            class="form-select"
                            on:change=move |ev| set_condition.set(event_target_value(&ev))
                        >
                            <option value="NM" selected=true>"NM — Near Mint"</option>
                            <option value="LP">"LP — Lightly Played"</option>
                            <option value="MP">"MP — Moderately Played"</option>
                            <option value="HP">"HP — Heavily Played"</option>
                            <option value="DMG">"DMG — Damaged"</option>
                        </select>
                    </div>

                    <div class="form-row">
                        <label class="form-label">"Quantity"</label>
                        <input
                            type="number"
                            class="form-input"
                            min="1"
                            prop:value=quantity
                            on:input=move |ev| {
                                if let Ok(n) = event_target_value(&ev).parse::<i32>() {
                                    set_quantity.set(n.max(1));
                                }
                            }
                        />
                    </div>
                })}

                <div class="form-row">
                    <label class="form-label">"Purchase price (optional)"</label>
                    <div class="input-prefix-wrap">
                        <span class="input-prefix">"$"</span>
                        <input
                            type="number"
                            step="0.01"
                            min="0"
                            class="form-input"
                            placeholder="0.00"
                            on:input=move |ev| set_cost_str.set(event_target_value(&ev))
                        />
                    </div>
                </div>

                <div class="form-row">
                    <label class="form-label">"Notes (optional)"</label>
                    <textarea
                        class="form-textarea"
                        rows="2"
                        placeholder="e.g. purchased at local shop"
                        on:input=move |ev| set_notes.set(event_target_value(&ev))
                    ></textarea>
                </div>

                {move || save_error.get().map(|e| view! {
                    <p class="error">"Save failed: "{e}</p>
                })}

                <div class="form-actions">
                    <button
                        type="submit"
                        class="btn-primary"
                        disabled=saving
                    >
                        {move || if saving.get() { "Saving…" } else { "Save to catalogue" }}
                    </button>
                </div>
            </form>
        </div>
    }
}

// ── API helpers ─────────────────────────────────────────────────────────────

async fn fetch_printings(query: &str) -> Result<SearchResults, String> {
    let encoded = js_sys::encode_uri_component(query);
    let encoded_str = encoded.as_string().unwrap_or_default();
    let url = format!("/api/printings/search?name={}", encoded_str);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json::<SearchResults>().await.map_err(|e| e.to_string())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}

async fn save_item(req: SaveRequest) -> Result<(), String> {
    let resp = gloo_net::http::Request::post("/api/catalogue/items")
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() || resp.status() == 201 {
        Ok(())
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(format!("HTTP {}: {}", resp.status(), body))
    }
}
