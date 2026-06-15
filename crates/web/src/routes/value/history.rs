//! `/value/:printing_id/history` — per-card price-history chart (Leptos 0.7 CSR).
//!
//! Fetches historical snapshots from `GET /api/valuations/history?printing_id=<id>`,
//! renders a sparkline SVG chart and a data table.
//! (leptos-chartistry not yet wired; uses a hand-built SVG sparkline instead.)

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api;

// ── Wire types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub printing_id: String,
    pub data: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub price_usd: f64,
    pub source: String,
    pub captured_at: String,
}

// ── API call ───────────────────────────────────────────────────────────────

pub async fn fetch_history(
    printing_id: &str,
    workspace_id: Uuid,
) -> Result<HistoryResponse, String> {
    api::get_json(&format!(
        "/valuations/history?printing_id={}&workspace_id={}&limit=90",
        printing_id, workspace_id
    ))
    .await
}

// ── Page component ─────────────────────────────────────────────────────────

/// Price-history page. Expects `:wid` and `:printing_id` in URL params.
#[component]
pub fn CardHistoryPage() -> impl IntoView {
    let params = use_params_map();
    let printing_id = move || params.with(|p| p.get("printing_id").unwrap_or_default());
    let workspace_id = move || {
        params.with(|p| {
            p.get("wid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
        })
    };

    let history = LocalResource::new(move || {
        let pid = printing_id();
        let wid = workspace_id();
        async move { fetch_history(&pid, wid?).await.ok() }
    });

    view! {
        <div class="p-6 space-y-6 max-w-4xl mx-auto">
            <nav class="text-sm">
                <a href="javascript:history.back()" class="text-blue-500 hover:underline">
                    "← Back"
                </a>
            </nav>
            <h1 class="text-2xl font-bold text-gray-900">"Price History"</h1>

            <Suspense fallback=move || view! { <ChartSkeleton /> }>
                {move || match history.get().as_deref() {
                    None => view! { <ChartSkeleton /> }.into_any(),
                    Some(None) => view! {
                        <div class="rounded-lg bg-red-50 border border-red-200 p-4 text-red-700">
                            <p class="font-semibold">"Failed to load price history"</p>
                        </div>
                    }.into_any(),
                    Some(Some(data)) if data.data.is_empty() => view! {
                        <EmptyHistoryState />
                    }.into_any(),
                    Some(Some(data)) => {
                        let data = data.clone();
                        view! {
                            <PriceHistoryContent points=data.data />
                        }.into_any()
                    },
                }}
            </Suspense>
        </div>
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────

#[component]
fn PriceHistoryContent(points: Vec<HistoryPoint>) -> impl IntoView {
    let mut sorted = points.clone();
    sorted.sort_by(|a, b| a.captured_at.cmp(&b.captured_at));

    let latest = sorted.last().cloned();
    let min_price = sorted.iter().map(|p| p.price_usd).fold(f64::MAX, f64::min);
    let max_price = sorted.iter().map(|p| p.price_usd).fold(f64::MIN, f64::max);

    view! {
        <div class="space-y-4">
            // Latest value banner
            {latest.map(|p| view! {
                <div class="bg-white rounded-xl shadow-sm border border-gray-100 p-4
                            flex items-center justify-between">
                    <div>
                        <p class="text-xs text-gray-500 uppercase tracking-wide">"Latest price"</p>
                        <p class="text-3xl font-bold text-green-600">
                            {format!("${:.2}", p.price_usd)}
                        </p>
                    </div>
                    <div class="text-right">
                        <p class="text-xs text-gray-400">"Source"</p>
                        <p class="text-sm font-medium text-gray-700">{p.source.clone()}</p>
                        <p class="text-xs text-gray-400">{p.captured_at.chars().take(10).collect::<String>()}</p>
                    </div>
                </div>
            })}

            // SVG sparkline chart
            <SparklineChart points=sorted.clone() min=min_price max=max_price />

            // Data table
            <div class="bg-white rounded-xl shadow-sm border border-gray-100 overflow-hidden">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="bg-gray-50 border-b text-left">
                            <th class="px-4 py-2 font-semibold text-gray-600">"Date"</th>
                            <th class="px-4 py-2 font-semibold text-gray-600 text-right">"Price"</th>
                            <th class="px-4 py-2 font-semibold text-gray-600">"Source"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-gray-50">
                        {sorted.iter().rev().map(|p| {
                            let date = p.captured_at.chars().take(10).collect::<String>();
                            let price_str = format!("${:.2}", p.price_usd);
                            let src = p.source.clone();
                            view! {
                                <tr class="hover:bg-gray-50">
                                    <td class="px-4 py-2 text-gray-500">{date}</td>
                                    <td class="px-4 py-2 text-right font-mono">{price_str}</td>
                                    <td class="px-4 py-2 text-gray-500">{src}</td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

/// Hand-built SVG sparkline — no external chart library dependency.
/// Uses a 600×120 viewBox; points normalized to the price range.
#[component]
fn SparklineChart(points: Vec<HistoryPoint>, min: f64, max: f64) -> impl IntoView {
    const W: f64 = 600.0;
    const H: f64 = 120.0;
    const PAD: f64 = 10.0;

    let range = (max - min).max(0.01); // avoid division by zero for flat price
    let n = points.len();

    let path_d = if n < 2 {
        String::new()
    } else {
        let mut parts = Vec::with_capacity(n);
        for (i, p) in points.iter().enumerate() {
            let x = PAD + (i as f64 / (n - 1) as f64) * (W - 2.0 * PAD);
            let y = H - PAD - ((p.price_usd - min) / range) * (H - 2.0 * PAD);
            parts.push(format!("{},{}", x, y));
        }
        format!("M {}", parts.join(" L "))
    };

    view! {
        <div class="bg-white rounded-xl shadow-sm border border-gray-100 p-4">
            <p class="text-sm font-semibold text-gray-700 mb-2">"90-Day Price Trend (USD)"</p>
            <svg viewBox=format!("0 0 {} {}", W, H) class="w-full h-32"
                 aria-label="Price history sparkline">
                // Y-axis labels
                <text x="4" y="14" class="text-xs fill-gray-400" font-size="9">
                    {format!("${:.2}", max)}
                </text>
                <text x="4" y=format!("{}", H - 2.0) class="text-xs fill-gray-400" font-size="9">
                    {format!("${:.2}", min)}
                </text>
                // Price line
                {if path_d.is_empty() { None } else { Some(view! {
                    <path d=path_d fill="none" stroke="#16a34a" stroke-width="2"
                          stroke-linecap="round" stroke-linejoin="round" />
                }) }}
                // Data points
                {points.iter().enumerate().map(|(i, p)| {
                    let x = PAD + (i as f64 / (n - 1).max(1) as f64) * (W - 2.0 * PAD);
                    let y = H - PAD - ((p.price_usd - min) / range) * (H - 2.0 * PAD);
                    view! {
                        <circle cx=x cy=y r="3" fill="#16a34a" />
                    }
                }).collect::<Vec<_>>()}
            </svg>
        </div>
    }
}

#[component]
fn EmptyHistoryState() -> impl IntoView {
    view! {
        <div class="text-center py-16 bg-white rounded-xl border border-gray-100">
            <p class="text-lg font-medium text-gray-600">"No price history yet"</p>
            <p class="mt-1 text-sm text-gray-400">
                "Price snapshots are recorded daily. Check back tomorrow."
            </p>
        </div>
    }
}

#[component]
fn ChartSkeleton() -> impl IntoView {
    view! {
        <div class="space-y-4 animate-pulse">
            <div class="h-20 bg-gray-200 rounded-xl"></div>
            <div class="h-40 bg-gray-200 rounded-xl"></div>
        </div>
    }
}
