use leptos::*;
use leptos_router::{use_params_map, A};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn sf(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::ServerError(e.to_string())
}

// ─── Shared types (mirrored from cardguard-api) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportView {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub report_date: String,
    pub status: String,
    pub discrepancy_count: i32,
    pub unresolved_count: i32,
    pub synced_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscrepancyView {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportDetail {
    pub report: ReportView,
    pub discrepancies: Vec<DiscrepancyView>,
}

// ─── Server functions ─────────────────────────────────────────────────────────

#[server(GetReportDetail, "/api")]
pub async fn get_report_detail(report_id: String) -> Result<ReportDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use cardguard_api::ReconciliationService;
        use leptos_axum::extract;
        use sqlx::PgPool;

        let pool = extract::<axum::Extension<PgPool>>()
            .await
            .map(|e| e.0)
            .map_err(sf)?;

        let rid = Uuid::parse_str(&report_id).map_err(|_| sf("Invalid report ID"))?;

        let report = ReconciliationService::get_report(&pool, rid)
            .await
            .map_err(sf)?;

        let discrepancies = ReconciliationService::get_discrepancies(&pool, rid)
            .await
            .map_err(sf)?;

        Ok(ReportDetail {
            report: ReportView {
                id: report.id,
                connection_id: report.connection_id,
                report_date: report.report_date.to_string(),
                status: report.status,
                discrepancy_count: report.discrepancy_count,
                unresolved_count: report.unresolved_count,
                synced_at: report.synced_at.map(|t| t.to_rfc3339()),
                error_message: report.error_message,
            },
            discrepancies: discrepancies
                .into_iter()
                .map(|d| DiscrepancyView {
                    id: d.id,
                    printing_id: d.printing_id,
                    pos_sku: d.pos_sku,
                    discrepancy_type: d.discrepancy_type,
                    catalogue_qty: d.catalogue_qty,
                    pos_qty: d.pos_qty,
                    resolution: d.resolution,
                    resolved_at: d.resolved_at.map(|t| t.to_rfc3339()),
                    notes: d.notes,
                })
                .collect(),
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(sf("SSR only"))
    }
}

#[server(ResolveDiscrepancy, "/api")]
pub async fn resolve_discrepancy_action(
    workspace_id: String,
    discrepancy_id: String,
    resolution: String,
    notes: Option<String>,
) -> Result<DiscrepancyView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use cardguard_api::ReconciliationService;
        use leptos_axum::extract;
        use sqlx::PgPool;

        let pool = extract::<axum::Extension<PgPool>>()
            .await
            .map(|e| e.0)
            .map_err(sf)?;

        let wid = Uuid::parse_str(&workspace_id).map_err(|_| sf("Invalid workspace ID"))?;
        let did = Uuid::parse_str(&discrepancy_id).map_err(|_| sf("Invalid discrepancy ID"))?;

        let disc = ReconciliationService::resolve_discrepancy(
            &pool,
            wid,
            did,
            &resolution,
            notes.as_deref(),
        )
        .await
        .map_err(sf)?;

        Ok(DiscrepancyView {
            id: disc.id,
            printing_id: disc.printing_id,
            pos_sku: disc.pos_sku,
            discrepancy_type: disc.discrepancy_type,
            catalogue_qty: disc.catalogue_qty,
            pos_qty: disc.pos_qty,
            resolution: disc.resolution,
            resolved_at: disc.resolved_at.map(|t| t.to_rfc3339()),
            notes: disc.notes,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(sf("SSR only"))
    }
}

// ─── Component helpers ────────────────────────────────────────────────────────

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

// ─── DiscrepancyRow component ─────────────────────────────────────────────────

#[component]
fn DiscrepancyRow(
    disc: DiscrepancyView,
    workspace_id: String,
    on_resolved: Action<(String, String, String, Option<String>), Result<DiscrepancyView, ServerFnError>>,
) -> impl IntoView {
    let disc = create_rw_signal(disc);
    let (notes_input, set_notes_input) = create_signal(String::new());

    let resolve = {
        let wid = workspace_id.clone();
        move |resolution: &'static str| {
            let d = disc.get();
            let did = d.id.to_string();
            let notes = notes_input.get();
            let notes_opt = if notes.is_empty() { None } else { Some(notes) };
            on_resolved.dispatch((wid.clone(), did, resolution.to_string(), notes_opt));
        }
    };

    // Update local state when the action completes for this disc.
    create_effect(move |_| {
        if let Some(Ok(updated)) = on_resolved.value().get() {
            if updated.id == disc.get().id {
                disc.set(updated);
            }
        }
    });

    view! {
        <tr class=move || if disc.get().resolution == "pending" { "row-pending" } else { "row-resolved" }>
            <td>
                <span class=move || disc_type_class(&disc.get().discrepancy_type)>
                    {move || disc_type_label(&disc.get().discrepancy_type)}
                </span>
            </td>
            <td class="sku-cell">{move || disc.get().pos_sku.clone()}</td>
            <td class="qty-cell">
                {move || disc.get().catalogue_qty.map(|q| q.to_string()).unwrap_or_else(|| "—".into())}
            </td>
            <td class="qty-cell">
                {move || disc.get().pos_qty.map(|q| q.to_string()).unwrap_or_else(|| "—".into())}
            </td>
            <td>
                {move || {
                    let current_resolution = disc.get().resolution;
                    if current_resolution == "pending" {
                        let r = resolve.clone();
                        view! {
                            <div class="resolve-actions">
                                <input
                                    type="text"
                                    placeholder="Notes (optional)"
                                    class="notes-input"
                                    on:input=move |ev| set_notes_input.set(event_target_value(&ev))
                                />
                                <div class="resolve-buttons">
                                    <button
                                        class="btn btn-sm btn-primary"
                                        on:click=move |_| r("accept_pos")
                                    >
                                        "Accept POS"
                                    </button>
                                    <button
                                        class="btn btn-sm btn-secondary"
                                        on:click={
                                            let r = resolve.clone();
                                            move |_| r("accept_catalogue")
                                        }
                                    >
                                        "Accept Catalogue"
                                    </button>
                                    <button
                                        class="btn btn-sm btn-ghost"
                                        on:click={
                                            let r = resolve.clone();
                                            move |_| r("manual_adjust")
                                        }
                                    >
                                        "Manual Adjust"
                                    </button>
                                    <button
                                        class="btn btn-sm btn-ghost"
                                        on:click={
                                            let r = resolve.clone();
                                            move |_| r("investigate")
                                        }
                                    >
                                        "Investigate"
                                    </button>
                                </div>
                            </div>
                        }.into_view()
                    } else {
                        view! {
                            <span class="resolution-label">
                                {resolution_label(&current_resolution)}
                            </span>
                        }.into_view()
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
    let report_id =
        move || params.with(|p| p.get("report_id").cloned().unwrap_or_default());

    // &'static str is Copy — safely captured by any number of closures.
    const WID: &str = "00000000-0000-0000-0000-000000000000";

    let detail = create_resource(report_id, |rid| async move { get_report_detail(rid).await });

    let resolve_action = create_action(
        |(wid, did, resolution, notes): &(String, String, String, Option<String>)| {
            let wid = wid.clone();
            let did = did.clone();
            let resolution = resolution.clone();
            let notes = notes.clone();
            async move { resolve_discrepancy_action(wid, did, resolution, notes).await }
        },
    );

    view! {
        <div class="reconcile-report">
            <div class="page-header">
                <A href="/reconcile">"← Back to Dashboard"</A>
                <h1>"Reconciliation Report"</h1>
            </div>

            <Suspense fallback=|| view! { <p class="loading">"Loading report…"</p> }>
                {move || {
                    detail.get().map(|res| match res {
                        Err(e) => view! {
                            <div class="error-banner">
                                <p>"Failed to load report: " {e.to_string()}</p>
                            </div>
                        }.into_view(),
                        Ok(d) => {
                            let report = d.report.clone();
                            let discs = d.discrepancies.clone();

                            let all_resolved = discs.iter().all(|d| d.resolution != "pending");
                            let unresolved_count = discs.iter().filter(|d| d.resolution == "pending").count();

                            view! {
                                <div>
                                    // Report meta
                                    <div class="report-meta">
                                        <div class="meta-row">
                                            <span class="meta-label">"Date"</span>
                                            <span>{report.report_date.clone()}</span>
                                        </div>
                                        <div class="meta-row">
                                            <span class="meta-label">"Status"</span>
                                            <span class=format!("badge {}", match report.status.as_str() {
                                                "completed" => "badge-success",
                                                "syncing" => "badge-info",
                                                "failed" | "stale" => "badge-error",
                                                _ => "badge-neutral",
                                            })>{report.status.clone()}</span>
                                        </div>
                                        {report.synced_at.clone().map(|t| view! {
                                            <div class="meta-row">
                                                <span class="meta-label">"Last synced"</span>
                                                <span>{t}</span>
                                            </div>
                                        })}
                                        {report.error_message.clone().map(|e| view! {
                                            <div class="meta-row error-row">
                                                <span class="meta-label">"Error"</span>
                                                <span class="error-text">{e}</span>
                                            </div>
                                        })}
                                    </div>

                                    // Progress towards zero discrepancies
                                    {if discs.is_empty() {
                                        view! {
                                            <div class="all-in-sync-banner">
                                                <span class="checkmark">"✓"</span>
                                                <strong>"All in sync"</strong>
                                                " — no discrepancies found."
                                            </div>
                                        }.into_view()
                                    } else if all_resolved {
                                        view! {
                                            <div class="all-resolved-banner">
                                                <span class="checkmark">"✓"</span>
                                                <strong>"Zero unexplained discrepancies"</strong>
                                                " — all items resolved."
                                            </div>
                                        }.into_view()
                                    } else {
                                        view! {
                                            <div class="progress-banner">
                                                <span>
                                                    {format!(
                                                        "{} of {} discrepanc{} resolved",
                                                        discs.len() - unresolved_count,
                                                        discs.len(),
                                                        if discs.len() == 1 { "y" } else { "ies" },
                                                    )}
                                                </span>
                                                <div class="progress-bar">
                                                    <div
                                                        class="progress-fill"
                                                        style=format!(
                                                            "width: {}%",
                                                            (discs.len() - unresolved_count) * 100 / discs.len()
                                                        )
                                                    />
                                                </div>
                                            </div>
                                        }.into_view()
                                    }}

                                    // Discrepancy table
                                    {if !discs.is_empty() {
                                        let action_clone = resolve_action;
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
                                                    <For
                                                        each=move || discs.clone()
                                                        key=|d| d.id
                                                        children=move |disc| {
                                                            view! {
                                                                <DiscrepancyRow
                                                                    disc=disc
                                                                    workspace_id=WID.to_string()
                                                                    on_resolved=action_clone
                                                                />
                                                            }
                                                        }
                                                    />
                                                </tbody>
                                            </table>
                                        }.into_view()
                                    } else {
                                        view! { <div/> }.into_view()
                                    }}
                                </div>
                            }.into_view()
                        }
                    })
                }}
            </Suspense>

            // Error toast when resolve action fails.
            {move || {
                resolve_action.value().get().and_then(|r| r.err()).map(|e| {
                    view! {
                        <div class="toast toast-error">
                            "Failed to resolve: " {e.to_string()}
                        </div>
                    }
                })
            }}
        </div>
    }
}
