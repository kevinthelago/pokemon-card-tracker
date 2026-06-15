//! `/value` — collection value overview page (Leptos 0.7 CSR).
//!
//! Fetches workspace valuations from `GET /api/valuations?workspace_id=<wid>`,
//! displays total collection value, a per-card table with value badges, and
//! a "Refresh Prices" button.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

use crate::api;

// ── Wire types (mirrors valuation_routes.rs JSON shapes) ──────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationsResponse {
    pub data: Vec<ValuationItem>,
    pub total_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationItem {
    pub printing_id: String,
    pub card_name: String,
    pub set_code: String,
    pub condition: String,
    pub quantity: i32,
    pub value: ItemValue,
    pub line_total_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ItemValue {
    Current { price_usd: f64, source: String, fetched_at: String },
    Stale { price_usd: f64, source: String, fetched_at: String, stale_seconds: i64 },
    NoData,
    Pending,
}

// ── API calls ──────────────────────────────────────────────────────────────

pub async fn fetch_valuations(workspace_id: Uuid) -> Result<ValuationsResponse, String> {
    api::get_json(&format!("/valuations?workspace_id={}", workspace_id)).await
}

pub async fn post_refresh(workspace_id: Uuid) -> Result<(), String> {
    api::post_void(&format!("/valuations/refresh?workspace_id={}", workspace_id)).await
}

// ── Page component ─────────────────────────────────────────────────────────

/// The value overview page. Expects `:wid` param in the URL.
#[component]
pub fn ValuePage() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| p.get("wid").and_then(|s| Uuid::parse_str(&s).ok()))
    };

    let (reload, set_reload) = signal(0u32);
    let (refreshing, set_refreshing) = signal(false);
    let (refresh_error, set_refresh_error) = signal(Option::<String>::None);

    let valuations = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move { fetch_valuations(wid?).await.ok() }
    });

    let handle_refresh = move |_| {
        let Some(wid) = workspace_id() else { return };
        set_refreshing.set(true);
        set_refresh_error.set(None);
        spawn_local(async move {
            match post_refresh(wid).await {
                Ok(()) => set_reload.update(|n| *n += 1),
                Err(e) => set_refresh_error.set(Some(e)),
            }
            set_refreshing.set(false);
        });
    };

    view! {
        <div class="p-6 space-y-6 max-w-6xl mx-auto">
            <div class="flex items-center justify-between">
                <h1 class="text-2xl font-bold text-gray-900">"Collection Value"</h1>
                <div class="flex items-center gap-3">
                    {move || refresh_error.get().map(|e| view! {
                        <span class="text-sm text-red-500">{e}</span>
                    })}
                    <button
                        class="px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded-lg
                               hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        disabled=move || refreshing.get()
                        on:click=handle_refresh
                    >
                        {move || if refreshing.get() { "Refreshing…" } else { "Refresh Prices" }}
                    </button>
                </div>
            </div>

            <Suspense fallback=move || view! { <LoadingSkeleton /> }>
                {move || match valuations.get().as_deref() {
                    None => view! { <LoadingSkeleton /> }.into_any(),
                    Some(None) => view! {
                        <div class="rounded-lg bg-red-50 border border-red-200 p-4 text-red-700">
                            <p class="font-semibold">"Failed to load valuations"</p>
                            <p class="text-sm">"Ensure you are signed in and the workspace ID is valid."</p>
                        </div>
                    }.into_any(),
                    Some(Some(data)) => {
                        let data = data.clone();
                        view! {
                            <CollectionTotalWidget total=data.total_usd count=data.data.len() />
                            <ValuationTable items=data.data />
                        }.into_any()
                    },
                }}
            </Suspense>
        </div>
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────

#[component]
fn CollectionTotalWidget(total: f64, count: usize) -> impl IntoView {
    view! {
        <div class="bg-white rounded-xl shadow-sm border border-gray-100 p-6">
            <p class="text-sm font-medium text-gray-500 uppercase tracking-wide">
                "Total Collection Value"
            </p>
            <p class="mt-1 text-4xl font-bold text-green-600">
                {format!("${:.2}", total)}
            </p>
            <p class="mt-1 text-xs text-gray-400">
                {format!("USD · {} item{} · Market prices (TCGplayer / PriceCharting)",
                    count, if count == 1 { "" } else { "s" })}
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
                    "Add cards through the catalogue to see their market values here."
                </p>
            </div>
        }.into_any();
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
                        <th class="px-4 py-3 font-semibold text-gray-600">"Status"</th>
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
    }.into_any()
}

#[component]
fn ValuationRow(item: ValuationItem) -> impl IntoView {
    let history_href = format!("/value/{}/history", item.printing_id);
    let card_name = item.card_name.clone();

    let (unit_price, badge_html, line_total) = match &item.value {
        ItemValue::Current { price_usd, source, .. } => (
            format!("${:.2}", price_usd),
            format!(r#"<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-green-100 text-green-800">{}</span>"#, source),
            item.line_total_usd.map(|t| format!("${:.2}", t)).unwrap_or_else(|| "—".into()),
        ),
        ItemValue::Stale { price_usd, source, .. } => (
            format!("${:.2}", price_usd),
            format!(r#"<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-yellow-100 text-yellow-800" title="Price data is stale">{} (stale)</span>"#, source),
            item.line_total_usd.map(|t| format!("${:.2}", t)).unwrap_or_else(|| "—".into()),
        ),
        ItemValue::NoData => (
            "—".into(),
            r#"<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-600">No market data</span>"#.into(),
            "—".into(),
        ),
        ItemValue::Pending => (
            "—".into(),
            r#"<span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-400">Pending</span>"#.into(),
            "—".into(),
        ),
    };

    view! {
        <tr class="hover:bg-gray-50 transition-colors">
            <td class="px-4 py-3">
                <a href=history_href class="font-medium text-blue-600 hover:underline">
                    {card_name}
                </a>
            </td>
            <td class="px-4 py-3 text-gray-500">{item.set_code}</td>
            <td class="px-4 py-3">{item.condition}</td>
            <td class="px-4 py-3 text-right tabular-nums">{item.quantity}</td>
            <td class="px-4 py-3 text-right font-mono tabular-nums">{unit_price}</td>
            <td class="px-4 py-3 text-right font-mono font-semibold tabular-nums">{line_total}</td>
            <td class="px-4 py-3">
                <span inner_html=badge_html></span>
            </td>
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
