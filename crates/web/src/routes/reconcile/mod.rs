pub mod mapping;
pub mod report;

// Re-export the dashboard under a path-compatible name so app.rs can import cleanly.
pub mod mod_route {
    pub use super::dashboard::ReconcileDashboard;
}

mod dashboard {
    use leptos::*;
    use leptos_router::A;
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    fn sf(e: impl std::fmt::Display) -> ServerFnError {
        ServerFnError::ServerError(e.to_string())
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DashboardSummary {
        pub reports: Vec<ReportRow>,
        pub unmapped_count: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReportRow {
        pub id: Uuid,
        pub connection_id: Uuid,
        pub connection_name: String,
        pub report_date: String,
        pub status: String,
        pub discrepancy_count: i32,
        pub unresolved_count: i32,
    }

    #[cfg(feature = "ssr")]
    pub async fn fetch_dashboard(workspace_id: Uuid) -> Result<DashboardSummary, ServerFnError> {
        use cardguard_api::ReconciliationService;
        use leptos_axum::extract;
        use sqlx::PgPool;

        let pool = extract::<axum::Extension<PgPool>>()
            .await
            .map(|e| e.0)
            .map_err(sf)?;

        let reports = ReconciliationService::list_reports(&pool, workspace_id, None)
            .await
            .map_err(sf)?;

        let unmapped_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM unmapped_pos_skus u \
             WHERE u.workspace_id = $1 \
               AND NOT EXISTS (\
                   SELECT 1 FROM pos_product_mappings m \
                   WHERE m.connection_id = u.connection_id AND m.pos_sku = u.pos_sku\
               )",
        )
        .bind(workspace_id)
        .fetch_one(&pool)
        .await
        .map_err(sf)?;

        let rows = reports
            .into_iter()
            .map(|r| ReportRow {
                id: r.id,
                connection_id: r.connection_id,
                connection_name: r.connection_id.to_string(), // connect-pos stream enriches this
                report_date: r.report_date.to_string(),
                status: r.status,
                discrepancy_count: r.discrepancy_count,
                unresolved_count: r.unresolved_count,
            })
            .collect();

        Ok(DashboardSummary {
            reports: rows,
            unmapped_count,
        })
    }

    #[server(GetDashboard, "/api")]
    pub async fn get_dashboard(workspace_id: String) -> Result<DashboardSummary, ServerFnError> {
        #[cfg(feature = "ssr")]
        {
            let wid = Uuid::parse_str(&workspace_id).map_err(|_| sf("Invalid workspace ID"))?;
            fetch_dashboard(wid).await
        }
        #[cfg(not(feature = "ssr"))]
        {
            Err(sf("SSR only"))
        }
    }

    // use_context is synchronous (no await) so the future of do_trigger_sync has
    // zero suspend points — trivially Send. leptos_axum::extract for dyn-trait types
    // trips Leptos 0.6's HRTB Send check, use_context avoids that entirely.
    #[cfg(feature = "ssr")]
    async fn do_trigger_sync(wid: Uuid, cid: Uuid) -> Result<String, ServerFnError> {
        use cardguard_api::ReconciliationService;

        let pool = leptos::use_context::<sqlx::PgPool>()
            .ok_or_else(|| sf("DB pool not in context"))?;

        let provider =
            leptos::use_context::<std::sync::Arc<dyn cardguard_api::PosProvider>>()
                .ok_or_else(|| sf("POS provider not in context"))?;

        tokio::task::spawn(async move {
            if let Err(e) =
                ReconciliationService::run_reconciliation(&pool, wid, cid, provider).await
            {
                tracing::error!(error = %e, "Background reconciliation failed");
            }
        });

        Ok("queued".to_string())
    }

    #[server(TriggerSync, "/api")]
    pub async fn trigger_sync(
        workspace_id: String,
        connection_id: String,
    ) -> Result<String, ServerFnError> {
        #[cfg(feature = "ssr")]
        {
            let wid = Uuid::parse_str(&workspace_id)
                .map_err(|_| sf("Invalid workspace ID"))?;
            let cid = Uuid::parse_str(&connection_id)
                .map_err(|_| sf("Invalid connection ID"))?;
            do_trigger_sync(wid, cid).await
        }
        #[cfg(not(feature = "ssr"))]
        {
            Err(sf("SSR only"))
        }
    }

    fn status_badge(status: &str) -> &'static str {
        match status {
            "completed" => "badge-success",
            "syncing" => "badge-info",
            "failed" | "stale" => "badge-error",
            _ => "badge-neutral",
        }
    }

    fn sync_state_label(summary: &DashboardSummary) -> &'static str {
        if summary.reports.iter().any(|r| r.status == "syncing") {
            return "Syncing…";
        }
        if summary.reports.iter().any(|r| r.status == "failed" || r.status == "stale") {
            return "Sync error / stale";
        }
        if summary.unmapped_count > 0 {
            return "Unmapped SKUs pending";
        }
        let total_unresolved: i32 = summary.reports.iter().map(|r| r.unresolved_count).sum();
        if total_unresolved == 0 {
            "All in sync"
        } else {
            "Has discrepancies"
        }
    }

    #[component]
    pub fn ReconcileDashboard() -> impl IntoView {
        // &'static str is Copy — safely captured by any number of closures.
        // In production this comes from the auth/session context.
        const WID: &str = "00000000-0000-0000-0000-000000000000";

        let summary = create_resource(
            move || WID.to_string(),
            |wid| async move { get_dashboard(wid).await },
        );

        let sync_action = create_action(|(wid, cid): &(String, String)| {
            let wid = wid.clone();
            let cid = cid.clone();
            async move { trigger_sync(wid, cid).await }
        });

        view! {
            <div class="reconcile-dashboard">
                <div class="dashboard-header">
                    <h1>"Inventory Reconciliation"</h1>
                    <Suspense fallback=|| view! { <span class="badge">"Loading…"</span> }>
                        {move || {
                            summary.get().map(|res| match res {
                                Ok(s) => {
                                    let label = sync_state_label(&s);
                                    let cls = match label {
                                        "All in sync" => "sync-state-badge state-ok",
                                        "Syncing…" => "sync-state-badge state-syncing",
                                        "Sync error / stale" => "sync-state-badge state-error",
                                        _ => "sync-state-badge state-warn",
                                    };
                                    view! { <span class=cls>{label}</span> }.into_view()
                                }
                                Err(e) => view! { <span class="state-error">{e.to_string()}</span> }.into_view(),
                            })
                        }}
                    </Suspense>
                </div>

                <Suspense fallback=|| view! { <p>"Loading reports…"</p> }>
                    {move || {
                        summary.get().map(|res| match res {
                            Err(e) => view! {
                                <div class="error-banner">
                                    <p>"Failed to load dashboard: " {e.to_string()}</p>
                                </div>
                            }.into_view(),
                            Ok(s) => {
                                let unmapped = s.unmapped_count;
                                let reports = s.reports.clone();
                                let is_empty = reports.is_empty();
                                view! {
                                    <div>
                                        {if unmapped > 0 {
                                            view! {
                                                <div class="unmapped-alert">
                                                    <span class="badge badge-warn">
                                                        {format!("{} unmapped SKU{}", unmapped, if unmapped == 1 { "" } else { "s" })}
                                                    </span>
                                                    <A href="/reconcile/mapping">" → Map them now"</A>
                                                </div>
                                            }.into_view()
                                        } else {
                                            view! { <span/> }.into_view()
                                        }}

                                        {if is_empty {
                                            view! {
                                                <p class="empty-state">
                                                    "No reconciliation reports yet. Connect a POS and run a sync to get started."
                                                </p>
                                            }.into_view()
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
                                                        <For
                                                            each=move || reports.clone()
                                                            key=|r| r.id
                                                            children=move |row| {
                                                                let report_id = row.id.to_string();
                                                                let href = format!("/reconcile/report/{}", report_id);
                                                                let badge = status_badge(&row.status);
                                                                let conn_id = row.connection_id.to_string();
                                                                let date = row.report_date.clone();
                                                                let conn_name = row.connection_name.clone();
                                                                let status = row.status.clone();
                                                                let disc_count = row.discrepancy_count;
                                                                let unresolved = row.unresolved_count;
                                                                view! {
                                                                    <tr>
                                                                        <td>{date}</td>
                                                                        <td>{conn_name}</td>
                                                                        <td>
                                                                            <span class=format!("badge {}", badge)>
                                                                                {status}
                                                                            </span>
                                                                        </td>
                                                                        <td>{disc_count}</td>
                                                                        <td>
                                                                            <span class=if unresolved == 0 { "resolved" } else { "unresolved" }>
                                                                                {unresolved}
                                                                            </span>
                                                                        </td>
                                                                        <td class="actions-cell">
                                                                            <A href=href>"View →"</A>
                                                                            <button
                                                                                class="btn btn-xs btn-ghost"
                                                                                on:click=move |_| {
                                                                                    sync_action.dispatch((
                                                                                        WID.to_string(),
                                                                                        conn_id.clone(),
                                                                                    ));
                                                                                }
                                                                            >
                                                                                "Sync Now"
                                                                            </button>
                                                                        </td>
                                                                    </tr>
                                                                }
                                                            }
                                                        />
                                                    </tbody>
                                                </table>
                                            }.into_view()
                                        }}
                                    </div>
                                }.into_view()
                            }
                        })
                    }}
                </Suspense>

                {move || {
                    if let Some(result) = sync_action.value().get() {
                        match result {
                            Ok(status) => view! {
                                <p class="sync-result">{format!("Sync complete: {}", status)}</p>
                            }.into_view(),
                            Err(e) => view! {
                                <p class="error">{format!("Sync failed: {}", e)}</p>
                            }.into_view(),
                        }
                    } else {
                        view! { <span/> }.into_view()
                    }
                }}
            </div>
        }
    }
}
