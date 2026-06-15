//! `/value` — collection value overview: total widget, per-card value badges, refresh button.

use chrono::{DateTime, Utc};
use leptos::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ── Shared API shapes (mirrors valuation_routes.rs) ───────────────────────
// These are duplicated here so the web crate compiles in WASM mode without
// pulling in API-only dependencies. They must stay in sync with the API response.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationsResponse {
    pub data: Vec<ValuationItem>,
    pub total_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationItem {
    pub printing_id: String,
    pub card_name: String,
    pub set_code: String,
    pub condition: String,
    pub quantity: i32,
    pub value: ItemValue,
    pub line_total_usd: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ItemValue {
    Current {
        price_usd: Decimal,
        source: String,
        fetched_at: DateTime<Utc>,
    },
    Stale {
        price_usd: Decimal,
        source: String,
        fetched_at: DateTime<Utc>,
        stale_seconds: i64,
    },
    NoData,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RefreshResponse {
    refreshed: u32,
}

// ── Server functions ───────────────────────────────────────────────────────

#[cfg(feature = "ssr")]
use crate::server::state::AppState;

#[server(GetValuations, "/api/v1")]
pub async fn get_valuations() -> Result<ValuationsResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use axum::extract::State;
        let state = expect_context::<AppState>();
        let claim = expect_context::<crate::server::auth::WorkspaceClaim>();

        let svc = state.valuation_service();

        let rows = svc
            .workspace_valuations(claim.workspace_id)
            .await
            .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

        let total = svc
            .workspace_total(claim.workspace_id)
            .await
            .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

        // Convert DB rows to wire shape
        let data = rows
            .into_iter()
            .map(|r| {
                let (value, line_total) = match (r.price, r.is_stale, r.fetched_at) {
                    (Some(price), false, Some(fetched_at)) => (
                        ItemValue::Current {
                            price_usd: price,
                            source: r.source.unwrap_or_default(),
                            fetched_at,
                        },
                        Some(price * rust_decimal::Decimal::from(r.quantity)),
                    ),
                    (Some(price), true, Some(fetched_at)) => (
                        ItemValue::Stale {
                            price_usd: price,
                            source: r.source.unwrap_or_default(),
                            fetched_at,
                            stale_seconds: (Utc::now() - fetched_at).num_seconds(),
                        },
                        Some(price * rust_decimal::Decimal::from(r.quantity)),
                    ),
                    (None, _, None) => (ItemValue::Pending, None),
                    _ => (ItemValue::NoData, None),
                };
                ValuationItem {
                    printing_id: r.printing_id,
                    card_name: r.card_name,
                    set_code: r.set_code,
                    condition: r.condition,
                    quantity: r.quantity,
                    value,
                    line_total_usd: line_total,
                }
            })
            .collect();

        Ok(ValuationsResponse { data, total_usd: total })
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(TriggerRefresh, "/api/v1")]
pub async fn trigger_refresh(
    printing_id: Option<String>,
) -> Result<RefreshResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<AppState>();
        let svc = state.valuation_service();

        let refreshed = if let Some(id) = printing_id {
            svc.refresh(&id)
                .await
                .map_err(|e| ServerFnError::ServerError(e.to_string()))?;
            1
        } else {
            svc.refresh_all()
                .await
                .map_err(|e| ServerFnError::ServerError(e.to_string()))?
        };

        Ok(RefreshResponse { refreshed })
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

// ── Page component ─────────────────────────────────────────────────────────

#[component]
pub fn ValuePage() -> impl IntoView {
    let valuations = create_resource(|| (), |_| get_valuations());
    let refresh_action = create_server_action::<TriggerRefresh>();

    // Re-fetch after a successful refresh
    create_effect(move |_| {
        if refresh_action.value().get().is_some() {
            valuations.refetch();
        }
    });

    view! {
        <div class="p-6 space-y-6 max-w-6xl mx-auto">
            <div class="flex items-center justify-between">
                <h1 class="text-2xl font-bold text-gray-900">"Collection Value"</h1>
                <ActionForm action=refresh_action>
                    <input type="hidden" name="printing_id" value="" />
                    <button
                        type="submit"
                        class="btn btn-primary btn-sm"
                        disabled=move || refresh_action.pending().get()
                    >
                        {move || if refresh_action.pending().get() {
                            "Refreshing…"
                        } else {
                            "Refresh Prices"
                        }}
                    </button>
                </ActionForm>
            </div>

            <Suspense fallback=move || view! { <LoadingSkeleton /> }>
                <ErrorBoundary fallback=|errors| view! {
                    <div class="rounded-lg bg-red-50 border border-red-200 p-4 text-red-700">
                        <p class="font-semibold">"Failed to load valuations"</p>
                        <For each=move || errors.get() key=|(k, _)| k.clone()
                            children=|(_, e)| view! { <p class="text-sm">{e.to_string()}</p> }
                        />
                    </div>
                }>
                    {move || valuations.get().map(|res| res.map(|data| view! {
                        <CollectionTotalWidget total=data.total_usd item_count=data.data.len() />
                        <ValuationTable items=data.data />
                    }))}
                </ErrorBoundary>
            </Suspense>
        </div>
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────

#[component]
fn CollectionTotalWidget(total: Decimal, item_count: usize) -> impl IntoView {
    view! {
        <div class="bg-white rounded-xl shadow-sm border border-gray-100 p-6">
            <p class="text-sm font-medium text-gray-500 uppercase tracking-wide">
                "Total Collection Value"
            </p>
            <p class="mt-1 text-4xl font-bold text-green-600">
                {format!("${:.2}", total)}
            </p>
            <p class="mt-1 text-xs text-gray-400">
                {format!("USD · {} item{}", item_count, if item_count == 1 { "" } else { "s" })}
                " · Market prices (TCGplayer / PriceCharting)"
            </p>
        </div>
    }
}

#[component]
fn ValuationTable(items: Vec<ValuationItem>) -> impl IntoView {
    if items.is_empty() {
        return view! {
            <div class="text-center py-16 text-gray-400">
                <p class="text-lg font-medium text-gray-600">"No cards in your collection"</p>
                <p class="mt-1 text-sm">
                    "Add cards through the "
                    <a href="/catalogue/add" class="text-blue-500 hover:underline">"catalogue"</a>
                    " to see their market values here."
                </p>
            </div>
        }.into_view();
    }

    view! {
        <div class="bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden">
            <table class="w-full text-sm">
                <thead>
                    <tr class="bg-gray-50 border-b border-gray-100 text-left">
                        <th class="px-4 py-3 font-semibold text-gray-600">"Card"</th>
                        <th class="px-4 py-3 font-semibold text-gray-600">"Set"</th>
                        <th class="px-4 py-3 font-semibold text-gray-600">"Condition"</th>
                        <th class="px-4 py-3 font-semibold text-gray-600 text-right">"Qty"</th>
                        <th class="px-4 py-3 font-semibold text-gray-600 text-right">"Unit Price"</th>
                        <th class="px-4 py-3 font-semibold text-gray-600 text-right">"Line Total"</th>
                        <th class="px-4 py-3 font-semibold text-gray-600">"Source"</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-50">
                    <For
                        each=move || items.clone()
                        key=|item| format!("{}-{}", item.printing_id, item.condition)
                        children=|item| view! { <ValuationRow item /> }
                    />
                </tbody>
            </table>
        </div>
    }
    .into_view()
}

#[component]
fn ValuationRow(item: ValuationItem) -> impl IntoView {
    let printing_id = item.printing_id.clone();
    let history_href = format!("/value/card/{}/history", printing_id);

    let (unit_price, source_badge, line_total) = match &item.value {
        ItemValue::Current { price_usd, source, .. } => (
            format!("${:.2}", price_usd),
            view! { <span class="badge badge-success text-xs">{source.clone()}</span> }.into_view(),
            item.line_total_usd.map(|t| format!("${:.2}", t)).unwrap_or_else(|| "—".into()),
        ),
        ItemValue::Stale { price_usd, source, .. } => (
            format!("${:.2}", price_usd),
            view! {
                <span class="badge badge-warning text-xs"
                      title="Price data is stale — refresh to update">
                    {source.clone()} " (stale)"
                </span>
            }
            .into_view(),
            item.line_total_usd.map(|t| format!("${:.2}", t)).unwrap_or_else(|| "—".into()),
        ),
        ItemValue::NoData => (
            "—".into(),
            view! { <span class="badge badge-ghost text-xs">"No market data"</span> }.into_view(),
            "—".into(),
        ),
        ItemValue::Pending => (
            "—".into(),
            view! { <span class="badge badge-ghost text-xs">"Pending"</span> }.into_view(),
            "—".into(),
        ),
    };

    view! {
        <tr class="hover:bg-gray-50 transition-colors">
            <td class="px-4 py-3">
                <a href=history_href class="font-medium text-blue-600 hover:underline">
                    {item.card_name}
                </a>
            </td>
            <td class="px-4 py-3 text-gray-500">{item.set_code}</td>
            <td class="px-4 py-3">{item.condition}</td>
            <td class="px-4 py-3 text-right tabular-nums">{item.quantity}</td>
            <td class="px-4 py-3 text-right font-mono tabular-nums">{unit_price}</td>
            <td class="px-4 py-3 text-right font-mono font-semibold tabular-nums">{line_total}</td>
            <td class="px-4 py-3">{source_badge}</td>
        </tr>
    }
}

#[component]
fn LoadingSkeleton() -> impl IntoView {
    view! {
        <div class="space-y-4 animate-pulse">
            <div class="h-24 bg-gray-200 rounded-xl"></div>
            <div class="h-64 bg-gray-200 rounded-xl"></div>
        </div>
    }
}
