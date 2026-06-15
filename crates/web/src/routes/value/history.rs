//! `/value/card/:printing_id/history` — per-card price-history chart.
//!
//! Uses `leptos-chartistry` for the 90-day line sparkline.
//! The chart is a client-side island (wasm); SSR renders a placeholder.

use chrono::{DateTime, Utc};
use leptos::*;
use leptos_router::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ── Wire types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub printing_id: String,
    pub data: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub price_usd: Decimal,
    pub source: String,
    /// RFC-3339 timestamp string (serde-friendly across SSR ↔ WASM boundary).
    pub captured_at: String,
}

// ── Server function ────────────────────────────────────────────────────────

#[server(GetCardHistory, "/api/v1")]
pub async fn get_card_history(
    printing_id: String,
    limit: Option<i64>,
) -> Result<HistoryResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = expect_context::<crate::server::state::AppState>();
        let _claim = expect_context::<crate::server::auth::WorkspaceClaim>();

        let svc = state.valuation_service();
        let lim = limit.unwrap_or(90).clamp(1, 365);

        let rows = svc
            .history(&printing_id, lim)
            .await
            .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

        let data = rows
            .into_iter()
            .map(|r| HistoryPoint {
                price_usd: r.price,
                source: r.source,
                captured_at: r.captured_at.to_rfc3339(),
            })
            .collect();

        Ok(HistoryResponse { printing_id, data })
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

// ── Page component ─────────────────────────────────────────────────────────

#[component]
pub fn CardHistoryPage() -> impl IntoView {
    let params = use_params_map();
    let printing_id =
        move || params.with(|p| p.get("printing_id").cloned().unwrap_or_default());

    let history = create_resource(printing_id, |id| get_card_history(id, Some(90)));

    view! {
        <div class="p-6 space-y-6 max-w-4xl mx-auto">
            <nav class="text-sm">
                <a href="/value" class="text-blue-500 hover:underline">"← Collection Value"</a>
            </nav>

            <Suspense fallback=move || view! { <ChartSkeleton /> }>
                <ErrorBoundary fallback=|errors| view! {
                    <div class="rounded-lg bg-red-50 border border-red-200 p-4 text-red-700">
                        <p class="font-semibold">"Failed to load price history"</p>
                        <For each=move || errors.get() key=|(k, _)| k.clone()
                            children=|(_, e)| view! { <p class="text-sm">{e.to_string()}</p> }
                        />
                    </div>
                }>
                    {move || history.get().map(|res| res.map(|data| view! {
                        <div class="space-y-6">
                            <div class="flex items-center justify-between">
                                <h1 class="text-2xl font-bold text-gray-900">
                                    "Price History"
                                </h1>
                                <p class="text-sm text-gray-400">
                                    {format!("{} data points", data.data.len())}
                                </p>
                            </div>

                            {if data.data.is_empty() {
                                view! { <EmptyHistoryState printing_id=data.printing_id /> }.into_view()
                            } else {
                                let latest = data.data.first().cloned();
                                view! {
                                    <LatestValueBanner point=latest />
                                    <PriceChart points=data.data />
                                }.into_view()
                            }}
                        </div>
                    }))}
                </ErrorBoundary>
            </Suspense>
        </div>
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────

#[component]
fn LatestValueBanner(point: Option<HistoryPoint>) -> impl IntoView {
    let Some(p) = point else {
        return view! { <div></div> }.into_view();
    };
    view! {
        <div class="bg-white rounded-xl shadow-sm border border-gray-100 p-4 flex items-center gap-4">
            <div>
                <p class="text-xs text-gray-500">"Latest price"</p>
                <p class="text-2xl font-bold text-green-600">
                    {format!("${:.2}", p.price_usd)}
                </p>
            </div>
            <div class="ml-auto text-right">
                <p class="text-xs text-gray-400">"Source"</p>
                <p class="text-sm font-medium">{p.source}</p>
            </div>
        </div>
    }
    .into_view()
}

/// Line chart of price over time using leptos-chartistry.
#[component]
fn PriceChart(points: Vec<HistoryPoint>) -> impl IntoView {
    // Sort oldest → newest for the chart x-axis
    let mut sorted = points.clone();
    sorted.sort_by(|a, b| a.captured_at.cmp(&b.captured_at));

    // Convert Decimal → f64 for the chart library
    let chart_data: Vec<(f64, f64)> = sorted
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let price: f64 = p.price_usd.try_into().unwrap_or(0.0);
            (i as f64, price)
        })
        .collect();

    // X-axis labels (date strings, sampled every N points for readability)
    let x_labels: Vec<String> = sorted
        .iter()
        .enumerate()
        .filter(|(i, _)| i % (sorted.len().max(1) / 6).max(1) == 0)
        .map(|(_, p)| p.captured_at[..10].to_string()) // "YYYY-MM-DD"
        .collect();

    view! {
        <div class="bg-white rounded-xl shadow-sm border border-gray-100 p-6">
            <h2 class="text-base font-semibold text-gray-700 mb-4">"90-Day Price Trend (USD)"</h2>
            // leptos-chartistry Chart component
            // API: Series::new maps data → y; x is index; line() adds a line series.
            // Adjust if the leptos-chartistry API version differs.
            <PriceChartInner data=chart_data x_labels />
        </div>
    }
}

/// Inner component isolates the chart import for easy swap if the crate API changes.
#[component]
fn PriceChartInner(data: Vec<(f64, f64)>, x_labels: Vec<String>) -> impl IntoView {
    use leptos_chartistry::*;

    let series = Series::new(|(_, y): &(f64, f64)| *y)
        .line(Line::new(|(x, _): &(f64, f64)| *x).with_name("Price (USD)"));

    view! {
        <Chart
            aspect_ratio=AspectRatio::from_outer_height(100.0, 280.0)
            series
            data
        />
    }
}

#[component]
fn EmptyHistoryState(printing_id: String) -> impl IntoView {
    view! {
        <div class="text-center py-16 text-gray-400 bg-white rounded-xl border border-gray-100">
            <svg class="mx-auto h-10 w-10 text-gray-300" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                      d="M7 12l3-3 3 3 4-4M8 21l4-4 4 4M3 4h18M4 4h16v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4z" />
            </svg>
            <p class="mt-3 text-base font-medium text-gray-600">"No price history yet"</p>
            <p class="mt-1 text-sm">
                "Price snapshots are recorded daily. Check back tomorrow."
            </p>
        </div>
    }
}

#[component]
fn ChartSkeleton() -> impl IntoView {
    view! {
        <div class="space-y-4 animate-pulse">
            <div class="h-16 bg-gray-200 rounded-xl"></div>
            <div class="h-64 bg-gray-200 rounded-xl"></div>
        </div>
    }
}
