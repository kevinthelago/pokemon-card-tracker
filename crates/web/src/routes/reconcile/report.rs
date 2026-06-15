use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};
use uuid::Uuid;

use crate::api;
use super::{DiscrepancyDto, ReportDetailDto};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn disc_type_label(t: &str) -> &'static str {
    match t {
        "missing" => "Missing from POS",
        "extra" => "Extra in POS",
        "quantity_mismatch" => "Qty mismatch",
        "negative_quantity" => "Negative qty",
        _ => "Unknown",
    }
}

fn disc_type_class(t: &str) -> &'static str {
    match t {
        "negative_quantity" => "disc-type disc-type--critical",
        "missing" | "extra" => "disc-type disc-type--warn",
        _ => "disc-type disc-type--info",
    }
}

fn resolution_label(r: &str) -> &'static str {
    match r {
        "pending" => "Pending",
        "accept_pos" => "Accepted POS qty",
        "accept_catalogue" => "Accepted catalogue qty",
        "manual_adjust" => "Manual adjustment",
        "investigate" => "Marked for investigation",
        _ => "Unknown",
    }
}

fn status_badge_class(status: &str) -> &'static str {
    match status {
        "completed" => "badge badge-success",
        "syncing" => "badge badge-info",
        "failed" | "stale" => "badge badge-error",
        _ => "badge badge-neutral",
    }
}

// ─── DiscrepancyRow component ─────────────────────────────────────────────────

#[component]
fn DiscrepancyRow(
    disc: DiscrepancyDto,
    workspace_id: Uuid,
    report_id: Uuid,
    on_resolved: Callback<DiscrepancyDto>,
) -> impl IntoView {
    let (current, set_current) = signal(disc);
    let (notes_input, set_notes_input) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let resolve = move |resolution: &'static str| {
        let d = current.get_untracked();
        if d.resolution != "pending" {
            return;
        }
        let notes = notes_input.get_untracked();
        let notes_opt = if notes.is_empty() { None } else { Some(notes) };
        set_busy.set(true);
        set_error.set(None);
        let disc_id = d.id;
        let on_resolved = on_resolved.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api::resolve_discrepancy(workspace_id, report_id, disc_id, resolution, notes_opt).await {
                Ok(updated) => {
                    set_current.set(updated.clone());
                    on_resolved.run(updated);
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    view! {
        <tr class=move || if current.get().resolution == "pending" { "row-pending" } else { "row-resolved" }>
            <td>
                <span class=move || disc_type_class(&current.get().discrepancy_type)>
                    {move || disc_type_label(&current.get().discrepancy_type)}
                </span>
            </td>
            <td class="sku-cell">{move || current.get().pos_sku.clone()}</td>
            <td class="qty-cell">
                {move || current.get().catalogue_qty.map(|q| q.to_string()).unwrap_or_else(|| "\u{2014}".into())}
            </td>
            <td class="qty-cell">
                {move || current.get().pos_qty.map(|q| q.to_string()).unwrap_or_else(|| "\u{2014}".into())}
            </td>
            <td>
                {move || {
                    if current.get().resolution == "pending" {
                        view! {
                            <div class="resolve-actions">
                                <input
                                    type="text"
                                    placeholder="Notes (optional)"
                                    class="notes-input"
                                    on:input=move |ev| set_notes_input.set(event_target_value(&ev))
                                    prop:disabled=move || busy.get()
                                />
                                <div class="resolve-buttons">
                                    <button
                                        class="btn btn-sm btn-primary"
                                        disabled=move || busy.get()
                                        on:click=move |_| resolve("accept_pos")
                                    >
                                        "Accept POS"
                                    </button>
                                    <button
                                        class="btn btn-sm btn-secondary"
                                        disabled=move || busy.get()
                                        on:click=move |_| resolve("accept_catalogue")
                                    >
                                        "Accept Catalogue"
                                    </button>
                                    <button
                                        class="btn btn-sm btn-ghost"
                                        disabled=move || busy.get()
                                        on:click=move |_| resolve("manual_adjust")
                                    >
                                        "Manual Adjust"
                                    </button>
                                    <button
                                        class="btn btn-sm btn-ghost"
                                        disabled=move || busy.get()
                                        on:click=move |_| resolve("investigate")
                                    >
                                        "Investigate"
                                    </button>
                                </div>
                                {move || error.get().map(|e| view! {
                                    <span class="error-text">{e}</span>
                                })}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <span class="resolution-label">
                                {move || resolution_label(&current.get().resolution)}
                            </span>
                        }.into_any()
                    }
                }}
            </td>
        </tr>
    }
}

// ─── ReconcileReportPage component ───────────────────────────────────────────

#[component]
pub fn ReconcileReportPage() -> impl IntoView {
    let params = use_params_map();

    let workspace_id = move || {
        params.with(|p| {
            p.get("wid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_default()
        })
    };
    let report_id = move || {
        params.with(|p| {
            p.get("rid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_default()
        })
    };

    let detail = LocalResource::new(move || {
        let wid = workspace_id();
        let rid = report_id();
        async move { api::fetch_reconcile_report(wid, rid).await.ok() }
    });

    // Track resolved count locally to show progress without a full refetch.
    let (extra_resolved, set_extra_resolved) = signal(0usize);

    let on_resolved = Callback::new(move |_updated: DiscrepancyDto| {
        set_extra_resolved.update(|n| *n += 1);
    });

    let back_href = move || format!("/workspaces/{}/reconcile", workspace_id());

    view! {
        <div class="reconcile-report">
            <div class="page-header">
                <A href=back_href>"\u{2190} Back to Dashboard"</A>
                <h1>"Reconciliation Report"</h1>
            </div>

            <Suspense fallback=|| view! { <p class="loading">"Loading report\u{2026}"</p> }>
                {move || {
                    match detail.get().map(|sw| sw.take()) {
                        None => view! { <p class="loading">"Loading\u{2026}"</p> }.into_any(),
                        Some(None) => view! {
                            <div class="error-banner"><p>"Failed to load report."</p></div>
                        }.into_any(),
                        Some(Some(ReportDetailDto { report, discrepancies })) => {
                            let total = discrepancies.len();
                            let initial_unresolved = discrepancies.iter().filter(|d| d.resolution == "pending").count();

                            let unresolved_count = move || {
                                initial_unresolved.saturating_sub(extra_resolved.get())
                            };
                            let resolved_count = move || total - unresolved_count();
                            let all_resolved = move || unresolved_count() == 0;

                            let report_date = report.report_date.clone();
                            let status = report.status.clone();
                            let badge = status_badge_class(&report.status);
                            let synced_at = report.synced_at.clone();
                            let error_message = report.error_message.clone();
                            let rid = report.id;

                            view! {
                                <div>
                                    <div class="report-meta">
                                        <div class="meta-row">
                                            <span class="meta-label">"Date"</span>
                                            <span>{report_date}</span>
                                        </div>
                                        <div class="meta-row">
                                            <span class="meta-label">"Status"</span>
                                            <span class=badge>{status}</span>
                                        </div>
                                        {synced_at.map(|t| view! {
                                            <div class="meta-row">
                                                <span class="meta-label">"Last synced"</span>
                                                <span>{t}</span>
                                            </div>
                                        })}
                                        {error_message.map(|e| view! {
                                            <div class="meta-row error-row">
                                                <span class="meta-label">"Error"</span>
                                                <span class="error-text">{e}</span>
                                            </div>
                                        })}
                                    </div>

                                    // Progress toward zero discrepancies
                                    {if total == 0 {
                                        view! {
                                            <div class="all-in-sync-banner">
                                                <span class="checkmark">"\u{2713}"</span>
                                                <strong>"All in sync"</strong>
                                                " \u{2014} no discrepancies found."
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="progress-banner">
                                                {move || if all_resolved() {
                                                    view! {
                                                        <div class="all-resolved-banner">
                                                            <span class="checkmark">"\u{2713}"</span>
                                                            <strong>"Zero unexplained discrepancies"</strong>
                                                            " \u{2014} all items resolved."
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div>
                                                            <span>
                                                                {move || format!(
                                                                    "{} of {} discrepanc{} resolved",
                                                                    resolved_count(),
                                                                    total,
                                                                    if total == 1 { "y" } else { "ies" },
                                                                )}
                                                            </span>
                                                            <div class="progress-bar">
                                                                <div
                                                                    class="progress-fill"
                                                                    style=move || format!(
                                                                        "width: {}%",
                                                                        if total > 0 { resolved_count() * 100 / total } else { 0 }
                                                                    )
                                                                />
                                                            </div>
                                                        </div>
                                                    }.into_any()
                                                }}
                                            </div>
                                        }.into_any()
                                    }}

                                    // Discrepancy table
                                    {if !discrepancies.is_empty() {
                                        let wid = workspace_id();
                                        let on_resolved_clone = on_resolved.clone();
                                        view! {
                                            <table class="discrepancy-table">
                                                <thead>
                                                    <tr>
                                                        <th>"Type"</th>
                                                        <th>"POS SKU"</th>
                                                        <th>"Catalogue Qty"</th>
                                                        <th>"POS Qty"</th>
                                                        <th>"Resolution"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {discrepancies.into_iter().map(|disc| {
                                                        let on_r = on_resolved_clone.clone();
                                                        view! {
                                                            <DiscrepancyRow
                                                                disc=disc
                                                                workspace_id=wid
                                                                report_id=rid
                                                                on_resolved=on_r
                                                            />
                                                        }
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        }.into_any()
                                    } else {
                                        view! { <div /> }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </Suspense>
        </div>
    }
}
