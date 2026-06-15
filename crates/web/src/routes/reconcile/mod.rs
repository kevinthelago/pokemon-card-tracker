pub mod mapping;
pub mod report;

pub use mapping::MappingQueuePage;
pub use report::ReconcileReportPage;

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api;

// ─── DTOs (mirror API response shapes) ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconciliationReportDto {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub report_date: String,
    pub status: String,
    pub discrepancy_count: i32,
    pub unresolved_count: i32,
    pub synced_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscrepancyDto {
    pub id: Uuid,
    pub printing_id: Option<Uuid>,
    pub pos_sku: String,
    pub discrepancy_type: String,
    pub catalogue_qty: Option<i32>,
    pub pos_qty: Option<i32>,
    pub resolution: String,
    pub resolved_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportDetailDto {
    pub report: ReconciliationReportDto,
    pub discrepancies: Vec<DiscrepancyDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingDto {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub printing_id: Uuid,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnmappedSkuDto {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub pos_product_name: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingQueueDto {
    pub mappings: Vec<MappingDto>,
    pub unmapped: Vec<UnmappedSkuDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingWithBackfillDto {
    pub mapping: MappingDto,
    pub backfilled_lines: i64,
}

// ─── Dashboard helpers ────────────────────────────────────────────────────────

fn status_badge_class(status: &str) -> &'static str {
    match status {
        "completed" => "badge badge-success",
        "syncing" => "badge badge-info",
        "failed" | "stale" => "badge badge-error",
        _ => "badge badge-neutral",
    }
}

fn overall_state_label(reports: &[ReconciliationReportDto], unmapped_count: usize) -> &'static str {
    if reports.iter().any(|r| r.status == "syncing") {
        return "Syncing\u{2026}";
    }
    if reports
        .iter()
        .any(|r| r.status == "failed" || r.status == "stale")
    {
        return "Sync error / stale";
    }
    if unmapped_count > 0 {
        return "Unmapped SKUs pending";
    }
    let total_unresolved: i32 = reports.iter().map(|r| r.unresolved_count).sum();
    if total_unresolved == 0 {
        "All in sync"
    } else {
        "Has discrepancies"
    }
}

fn state_label_class(label: &str) -> &'static str {
    match label {
        "All in sync" => "sync-state-badge state-ok",
        "Syncing\u{2026}" => "sync-state-badge state-syncing",
        "Sync error / stale" => "sync-state-badge state-error",
        _ => "sync-state-badge state-warn",
    }
}

// ─── ReconcileDashboard component ─────────────────────────────────────────────

#[component]
pub fn ReconcileDashboard() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| {
            p.get("wid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_default()
        })
    };

    let (reload, set_reload) = signal(0u32);

    let reports = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move {
            api::fetch_reconcile_reports(wid)
                .await
                .ok()
                .unwrap_or_default()
        }
    });

    let mapping_queue = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move { api::fetch_mapping_queue(wid).await.ok() }
    });

    let (sync_error, set_sync_error) = signal(Option::<String>::None);
    let (sync_pending, set_sync_pending) = signal(false);

    let do_sync = move |connection_id: Uuid| {
        let wid = workspace_id();
        set_sync_pending.set(true);
        set_sync_error.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            match api::trigger_sync(wid, connection_id).await {
                Ok(_) => set_reload.update(|n| *n += 1),
                Err(e) => set_sync_error.set(Some(e)),
            }
            set_sync_pending.set(false);
        });
    };

    view! {
        <div class="reconcile-dashboard">
            <div class="dashboard-header">
                <h1>"Inventory Reconciliation"</h1>

                <Suspense fallback=|| view! { <span class="badge">"Loading\u{2026}"</span> }>
                    {move || {
                        let rpts = reports.get();
                        let queue = mapping_queue.get();
                        match (rpts, queue) {
                            (Some(rpts), Some(queue)) => {
                                let unmapped_count = queue.as_ref().map(|q| q.unmapped.len()).unwrap_or(0);
                                let label = overall_state_label(&rpts, unmapped_count);
                                view! {
                                    <span class=state_label_class(label)>{label}</span>
                                }.into_any()
                            }
                            _ => view! { <span class="badge">"Loading\u{2026}"</span> }.into_any(),
                        }
                    }}
                </Suspense>
            </div>

            <Suspense fallback=|| view! { <p>"Loading reports\u{2026}"</p> }>
                {move || {
                    let rpts = reports.get();
                    let queue = mapping_queue.get();
                    match (rpts, queue) {
                        (None, _) | (_, None) => view! {
                            <p>"Loading\u{2026}"</p>
                        }.into_any(),
                        (Some(rpts), Some(queue)) => {
                            let unmapped_count = queue.as_ref().map(|q| q.unmapped.len()).unwrap_or(0);
                            let mapping_href = format!("/workspaces/{}/reconcile/mapping", workspace_id());
                            let is_empty = rpts.is_empty();
                            let rpts_clone = (*rpts).clone();
                            view! {
                                <div>
                                    {if unmapped_count > 0 {
                                        view! {
                                            <div class="unmapped-alert">
                                                <span class="badge badge-warn">
                                                    {format!("{} unmapped SKU{}", unmapped_count, if unmapped_count == 1 { "" } else { "s" })}
                                                </span>
                                                <A href=mapping_href>" \u{2192} Map them now"</A>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <span /> }.into_any()
                                    }}

                                    {if is_empty {
                                        view! {
                                            <p class="empty-state">
                                                "No reconciliation reports yet. Connect a POS and run a sync to get started."
                                            </p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <table class="report-table">
                                                <thead>
                                                    <tr>
                                                        <th>"Date"</th>
                                                        <th>"Connection"</th>
                                                        <th>"Status"</th>
                                                        <th>"Discrepancies"</th>
                                                        <th>"Unresolved"</th>
                                                        <th></th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {rpts_clone.into_iter().map(|row| {
                                                        let wid = workspace_id();
                                                        let href = format!("/workspaces/{}/reconcile/report/{}", wid, row.id);
                                                        let badge = status_badge_class(&row.status);
                                                        let conn_id = row.connection_id;
                                                        let do_sync_clone = do_sync.clone();
                                                        view! {
                                                            <tr>
                                                                <td>{row.report_date.clone()}</td>
                                                                <td class="mono">{row.connection_id.to_string()}</td>
                                                                <td>
                                                                    <span class=badge>{row.status.clone()}</span>
                                                                </td>
                                                                <td>{row.discrepancy_count}</td>
                                                                <td>
                                                                    <span class=if row.unresolved_count == 0 { "resolved" } else { "unresolved" }>
                                                                        {row.unresolved_count}
                                                                    </span>
                                                                </td>
                                                                <td class="actions-cell">
                                                                    <A href=href>"View \u{2192}"</A>
                                                                    <button
                                                                        class="btn btn-xs btn-ghost"
                                                                        disabled=move || sync_pending.get()
                                                                        on:click=move |_| do_sync_clone(conn_id)
                                                                    >
                                                                        "Sync Now"
                                                                    </button>
                                                                </td>
                                                            </tr>
                                                        }
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </Suspense>

            {move || {
                sync_error.get().map(|e| view! {
                    <div class="toast toast-error">"Sync failed: " {e}</div>
                })
            }}
        </div>
    }
}
