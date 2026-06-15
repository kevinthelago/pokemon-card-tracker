use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use crate::api;

use super::{FlagKind, FlagSeverity, FlagStatus, RiskFlag, RiskSummary};

// ─── Page entry-point ─────────────────────────────────────────────────────────

#[component]
pub fn RiskDashboardPage() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| p.get("wid").as_deref().and_then(|s| Uuid::parse_str(s).ok()))
    };

    let (reload, set_reload) = signal(0u32);
    let trigger_reload = move || set_reload.update(|n| *n += 1);

    // Filters.
    let (filter_kind, set_filter_kind) = signal(Option::<FlagKind>::None);
    let (filter_severity, set_filter_severity) = signal(Option::<FlagSeverity>::None);
    let (filter_status, set_filter_status) = signal(Some(FlagStatus::Open));
    let (cursor, set_cursor) = signal(Option::<String>::None);

    // Selected flag for the detail drawer.
    let (selected_flag, set_selected_flag) = signal(Option::<RiskFlag>::None);

    // Bulk-selection state.
    let (selected_ids, set_selected_ids) = signal(Vec::<Uuid>::new());

    let summary = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move {
            let wid = wid?;
            api::fetch_risk_summary(wid).await.ok()
        }
    });

    let flags_page = LocalResource::new(move || {
        let wid = workspace_id();
        let kind = filter_kind.get();
        let severity = filter_severity.get();
        let status = filter_status.get();
        let after = cursor.get();
        let _ = reload.get();
        async move {
            let wid = wid?;
            api::fetch_risk_flags(wid, kind, severity, status, after).await.ok()
        }
    });

    view! {
        <div class="risk-dashboard">
            <header class="risk-dashboard__header">
                <h1 class="risk-dashboard__title">"Risk Dashboard"</h1>
                <p class="risk-dashboard__subtitle">
                    "Triage fraud alerts for your workspace."
                </p>
            </header>

            // ── Summary widgets ──────────────────────────────────────────────
            <Suspense fallback=move || view! { <SummarySkeletons /> }>
                {move || {
                    match summary.get().as_deref() {
                        Some(Some(s)) => view! { <SummaryWidgets summary=s.clone() /> }.into_any(),
                        _ => view! { <SummarySkeletons /> }.into_any(),
                    }
                }}
            </Suspense>

            // ── Filter bar ───────────────────────────────────────────────────
            <div class="risk-filters">
                <FilterSelect
                    label="Kind"
                    options=vec![
                        ("", "All kinds"),
                        ("stolen_card", "Stolen card"),
                        ("scalper", "Scalper"),
                        ("counterfeit", "Counterfeit"),
                    ]
                    on_change=move |v: String| {
                        set_cursor.set(None);
                        set_selected_ids.set(vec![]);
                        set_filter_kind.set(match v.as_str() {
                            "stolen_card" => Some(FlagKind::StolenCard),
                            "scalper"     => Some(FlagKind::Scalper),
                            "counterfeit" => Some(FlagKind::Counterfeit),
                            _             => None,
                        });
                    }
                />
                <FilterSelect
                    label="Severity"
                    options=vec![
                        ("", "All severities"),
                        ("critical", "Critical"),
                        ("high", "High"),
                        ("medium", "Medium"),
                        ("low", "Low"),
                    ]
                    on_change=move |v: String| {
                        set_cursor.set(None);
                        set_selected_ids.set(vec![]);
                        set_filter_severity.set(match v.as_str() {
                            "critical" => Some(FlagSeverity::Critical),
                            "high"     => Some(FlagSeverity::High),
                            "medium"   => Some(FlagSeverity::Medium),
                            "low"      => Some(FlagSeverity::Low),
                            _          => None,
                        });
                    }
                />
                <FilterSelect
                    label="Status"
                    options=vec![
                        ("open", "Open"),
                        ("reviewed", "Reviewed"),
                        ("dismissed", "Dismissed"),
                        ("", "All statuses"),
                    ]
                    on_change=move |v: String| {
                        set_cursor.set(None);
                        set_selected_ids.set(vec![]);
                        set_filter_status.set(match v.as_str() {
                            "open"      => Some(FlagStatus::Open),
                            "reviewed"  => Some(FlagStatus::Reviewed),
                            "dismissed" => Some(FlagStatus::Dismissed),
                            _           => None,
                        });
                    }
                />
            </div>

            // ── Bulk actions bar (shown when items are selected) ─────────────
            {move || {
                let ids = selected_ids.get();
                if ids.is_empty() {
                    return view! { <span /> }.into_any();
                }
                let wid = workspace_id().unwrap_or_default();
                let ids_for_review = ids.clone();
                let ids_for_dismiss = ids.clone();
                let tr = trigger_reload;
                let tr2 = trigger_reload;
                view! {
                    <div class="risk-bulk-bar">
                        <span class="risk-bulk-bar__count">
                            {ids.len()} " selected"
                        </span>
                        <BulkButton
                            label="Mark reviewed"
                            flag_ids=ids_for_review
                            status="reviewed"
                            workspace_id=wid
                            on_done=Callback::new(move |_: ()| {
                                set_selected_ids.set(vec![]);
                                tr();
                            })
                        />
                        <BulkButton
                            label="Dismiss"
                            flag_ids=ids_for_dismiss
                            status="dismissed"
                            workspace_id=wid
                            on_done=Callback::new(move |_: ()| {
                                set_selected_ids.set(vec![]);
                                tr2();
                            })
                        />
                    </div>
                }
                .into_any()
            }}

            // ── Flag table ───────────────────────────────────────────────────
            <Suspense fallback=move || view! { <p class="loading">"Loading flags\u{2026}"</p> }>
                {move || {
                    match flags_page.get().as_deref() {
                        None => view! { <p class="loading">"Loading\u{2026}"</p> }.into_any(),
                        Some(None) => {
                            view! {
                                <p class="error">"Failed to load flags. Please refresh."</p>
                            }
                            .into_any()
                        }
                        Some(Some(page)) => {
                            if page.items.is_empty() {
                                view! { <AllClearState /> }.into_any()
                            } else {
                                let next_cursor = page.next_cursor.clone();
                                view! {
                                    <FlagTable
                                        flags=page.items.clone()
                                        selected_ids=selected_ids
                                        on_select=Callback::new(move |id: Uuid| {
                                            set_selected_ids.update(|ids| {
                                                if ids.contains(&id) {
                                                    ids.retain(|&x| x != id);
                                                } else {
                                                    ids.push(id);
                                                }
                                            });
                                        })
                                        on_open=Callback::new(move |flag: RiskFlag| {
                                            set_selected_flag.set(Some(flag));
                                        })
                                    />
                                    {next_cursor.map(|nc| view! {
                                        <button
                                            class="btn btn--ghost risk-load-more"
                                            on:click=move |_| set_cursor.set(Some(nc.clone()))
                                        >
                                            "Load more"
                                        </button>
                                    })}
                                }
                                .into_any()
                            }
                        }
                    }
                }}
            </Suspense>

            // ── Detail drawer (slide-in when a flag is selected) ─────────────
            {move || {
                selected_flag.get().map(|flag| {
                    let wid = flag.workspace_id;
                    let tr = trigger_reload;
                    view! {
                        <FlagDrawer
                            flag=flag
                            workspace_id=wid
                            on_close=Callback::new(move |_: ()| set_selected_flag.set(None))
                            on_triaged=Callback::new(move |_: ()| {
                                set_selected_flag.set(None);
                                tr();
                            })
                        />
                    }
                })
            }}
        </div>
    }
}

// ─── Summary widgets ──────────────────────────────────────────────────────────

#[component]
fn SummaryWidgets(summary: RiskSummary) -> impl IntoView {
    let open_total: i64 = summary.counts_by_severity.iter().map(|c| c.open).sum();
    let resolved_total: i64 =
        summary.counts_by_severity.iter().map(|c| c.total - c.open).sum();
    let grand_total = open_total + resolved_total;

    view! {
        <div class="risk-summary">
            <div class="risk-summary__cards">
                <div class="risk-stat-card risk-stat-card--open">
                    <span class="risk-stat-card__label">"Open"</span>
                    <span class="risk-stat-card__value">{open_total}</span>
                </div>
                <div class="risk-stat-card risk-stat-card--resolved">
                    <span class="risk-stat-card__label">"Resolved"</span>
                    <span class="risk-stat-card__value">{resolved_total}</span>
                </div>
                <div class="risk-stat-card">
                    <span class="risk-stat-card__label">"Total"</span>
                    <span class="risk-stat-card__value">{grand_total}</span>
                </div>
            </div>

            <div class="risk-summary__severity">
                <h3 class="risk-summary__section-title">"By severity"</h3>
                {summary.counts_by_severity.into_iter().map(|c| {
                    let pct = if c.total > 0 {
                        (c.open as f64 / c.total as f64 * 100.0) as u32
                    } else {
                        0
                    };
                    let label = c.severity.label();
                    let css = c.severity.css_class();
                    view! {
                        <div class=format!("severity-bar {css}")>
                            <span class="severity-bar__label">{label}</span>
                            <div class="severity-bar__track">
                                <div
                                    class="severity-bar__fill"
                                    style=format!("width:{pct}%")
                                />
                            </div>
                            <span class="severity-bar__count">
                                {c.open} "/" {c.total}
                            </span>
                        </div>
                    }
                }).collect_view()}
            </div>

            <div class="risk-summary__trend">
                <h3 class="risk-summary__section-title">"Trend (30 days)"</h3>
                <TrendChart points=summary.trend />
            </div>
        </div>
    }
}

#[component]
fn SummarySkeletons() -> impl IntoView {
    view! {
        <div class="risk-summary risk-summary--loading">
            <div class="risk-summary__cards">
                <div class="risk-stat-card skeleton" />
                <div class="risk-stat-card skeleton" />
                <div class="risk-stat-card skeleton" />
            </div>
        </div>
    }
}

// leptos-chartistry 0.2.x requires Leptos 0.8; this project is on 0.7, so
// the trend chart uses a CSS-based bar implementation instead.
#[component]
fn TrendChart(points: Vec<super::TrendPoint>) -> impl IntoView {
    let max_val = points
        .iter()
        .flat_map(|p| [p.opened, p.resolved])
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    view! {
        <div class="trend-chart" role="img" aria-label="Open vs resolved trend">
            {points.into_iter().map(|p| {
                let open_h = (p.opened as f64 / max_val * 100.0) as u32;
                let res_h  = (p.resolved as f64 / max_val * 100.0) as u32;
                let day    = p.day.format("%m/%d").to_string();
                view! {
                    <div class="trend-chart__col" title=day>
                        <div
                            class="trend-chart__bar trend-chart__bar--opened"
                            style=format!("height:{open_h}%")
                        />
                        <div
                            class="trend-chart__bar trend-chart__bar--resolved"
                            style=format!("height:{res_h}%")
                        />
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

// ─── Flag table ───────────────────────────────────────────────────────────────

#[component]
fn FlagTable(
    flags: Vec<RiskFlag>,
    selected_ids: ReadSignal<Vec<Uuid>>,
    on_select: Callback<Uuid>,
    on_open: Callback<RiskFlag>,
) -> impl IntoView {
    view! {
        <table class="risk-table">
            <thead>
                <tr>
                    <th class="risk-table__check" />
                    <th>"Severity"</th>
                    <th>"Kind"</th>
                    <th>"Title"</th>
                    <th>"Status"</th>
                    <th>"Created"</th>
                    <th />
                </tr>
            </thead>
            <tbody>
                {flags.into_iter().map(|flag| {
                    let flag_id = flag.id;
                    let flag_for_open = flag.clone();
                    view! {
                        <FlagRow
                            flag=flag
                            is_selected=Signal::derive(move || selected_ids.get().contains(&flag_id))
                            on_select=Callback::new(move |_: leptos::ev::MouseEvent| {
                                on_select.run(flag_id);
                            })
                            on_open=Callback::new({
                                let flag = flag_for_open.clone();
                                move |_: leptos::ev::MouseEvent| on_open.run(flag.clone())
                            })
                        />
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}

#[component]
fn FlagRow(
    flag: RiskFlag,
    is_selected: Signal<bool>,
    on_select: Callback<leptos::ev::MouseEvent>,
    on_open: Callback<leptos::ev::MouseEvent>,
) -> impl IntoView {
    let sev_label    = flag.severity.label();
    let badge_class  = format!("badge {}", flag.severity.css_class());
    let kind_label   = flag.kind.label();
    let title        = flag.title.clone();
    let status_label = flag.status.label();
    let day          = flag.created_at.format("%Y-%m-%d").to_string();

    let row_class = move || {
        if is_selected.get() {
            "risk-table__row risk-table__row--selected"
        } else {
            "risk-table__row"
        }
    };

    view! {
        <tr class=row_class>
            <td>
                // Use on:click to toggle; visual selection is driven by the row class.
                <input
                    type="checkbox"
                    checked=move || is_selected.get()
                    on:click=move |e| on_select.run(e)
                />
            </td>
            <td><span class=badge_class>{sev_label}</span></td>
            <td>{kind_label}</td>
            <td class="risk-table__title">{title}</td>
            <td>{status_label}</td>
            <td class="risk-table__date">{day}</td>
            <td>
                <button
                    class="btn btn--sm btn--ghost"
                    on:click=move |e| on_open.run(e)
                >
                    "View"
                </button>
            </td>
        </tr>
    }
}

// ─── Detail drawer ────────────────────────────────────────────────────────────

#[component]
fn FlagDrawer(
    flag: RiskFlag,
    workspace_id: Uuid,
    on_close: Callback<()>,
    on_triaged: Callback<()>,
) -> impl IntoView {
    let (busy, set_busy) = signal(false);
    let (err, set_err) = signal(Option::<String>::None);

    let flag_id = flag.id;
    let is_open = matches!(flag.status, FlagStatus::Open);
    let evidence_str =
        serde_json::to_string_pretty(&flag.evidence).unwrap_or_else(|_| "{}".into());

    let make_triage_handler = |status: &'static str| {
        let on_triaged = on_triaged.clone();
        move |_: leptos::ev::MouseEvent| {
            set_busy.set(true);
            set_err.set(None);
            let on_triaged = on_triaged.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api::triage_flag(workspace_id, flag_id, status).await {
                    Ok(_) => on_triaged.run(()),
                    Err(e) => set_err.set(Some(format!("Error: {e}"))),
                }
                set_busy.set(false);
            });
        }
    };

    let on_reviewed = make_triage_handler("reviewed");
    let on_dismissed = make_triage_handler("dismissed");

    view! {
        <div class="flag-drawer-overlay" on:click=move |_| on_close.run(()) />
        <aside class="flag-drawer">
            <div class="flag-drawer__header">
                <h2 class="flag-drawer__title">{flag.title.clone()}</h2>
                <button
                    class="flag-drawer__close btn btn--ghost btn--sm"
                    on:click=move |_| on_close.run(())
                >
                    "\u{00D7}"
                </button>
            </div>

            <dl class="flag-drawer__meta">
                <dt>"Kind"</dt>
                <dd>{flag.kind.label()}</dd>
                <dt>"Severity"</dt>
                <dd>
                    <span class=format!("badge {}", flag.severity.css_class())>
                        {flag.severity.label()}
                    </span>
                </dd>
                <dt>"Status"</dt>
                <dd>{flag.status.label()}</dd>
                <dt>"Target"</dt>
                <dd>
                    {format!("{:?}", flag.target_type).to_lowercase()}
                    " \u{00B7} "
                    {flag.target_id.to_string()}
                </dd>
                <dt>"Created"</dt>
                <dd>{flag.created_at.format("%Y-%m-%d %H:%M UTC").to_string()}</dd>
            </dl>

            <section class="flag-drawer__evidence">
                <h3>"Evidence"</h3>
                <pre class="flag-drawer__evidence-json">{evidence_str}</pre>
            </section>

            {move || err.get().map(|e| view! { <p class="error flag-drawer__error">{e}</p> })}

            {is_open.then(|| view! {
                <div class="flag-drawer__actions">
                    <button
                        class="btn btn--primary"
                        disabled=move || busy.get()
                        on:click=on_reviewed
                    >
                        "Mark reviewed"
                    </button>
                    <button
                        class="btn btn--ghost"
                        disabled=move || busy.get()
                        on:click=on_dismissed
                    >
                        "Dismiss"
                    </button>
                </div>
            })}
        </aside>
    }
}

// ─── Bulk action button ───────────────────────────────────────────────────────

#[component]
fn BulkButton(
    label: &'static str,
    flag_ids: Vec<Uuid>,
    status: &'static str,
    workspace_id: Uuid,
    on_done: Callback<()>,
) -> impl IntoView {
    let (busy, set_busy) = signal(false);

    view! {
        <button
            class="btn btn--sm btn--ghost"
            disabled=move || busy.get()
            on:click=move |_| {
                set_busy.set(true);
                let ids = flag_ids.clone();
                let on_done = on_done.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = api::bulk_triage_flags(workspace_id, &ids, status).await;
                    on_done.run(());
                    set_busy.set(false);
                });
            }
        >
            {label}
        </button>
    }
}

// ─── All-clear empty state ────────────────────────────────────────────────────

#[component]
fn AllClearState() -> impl IntoView {
    view! {
        <div class="risk-all-clear">
            <div class="risk-all-clear__icon" aria-hidden="true">"\u{2705}"</div>
            <h2 class="risk-all-clear__heading">"All clear"</h2>
            <p class="risk-all-clear__body">
                "No risk flags match the current filters."
            </p>
        </div>
    }
}

// ─── Generic filter select ────────────────────────────────────────────────────

#[component]
fn FilterSelect(
    label: &'static str,
    options: Vec<(&'static str, &'static str)>,
    on_change: impl Fn(String) + 'static,
) -> impl IntoView {
    view! {
        <label class="risk-filter">
            <span class="risk-filter__label">{label}</span>
            <select
                class="risk-filter__select"
                on:change=move |ev| on_change(event_target_value(&ev))
            >
                {options.into_iter().map(|(val, text)| {
                    view! { <option value=val>{text}</option> }
                }).collect_view()}
            </select>
        </label>
    }
}
