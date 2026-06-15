//! "My Reports" screen — list of stolen-card reports submitted by the current user.
//!
//! Shows the moderation status of each report with appropriate contextual guidance
//! for each state: pending, confirmed, disputed, rejected, resolved.

use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub id: String,
    pub grader: String,
    pub cert_number: String,
    pub scope: String,
    pub status: String,
    pub evidence: Option<String>,
    pub confirmed_at: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyReportsPage {
    pub data: Vec<ReportSummary>,
    pub next_cursor: Option<String>,
}

// ─── Server function ──────────────────────────────────────────────────────────

#[server(FetchMyReports, "/api")]
pub async fn fetch_my_reports(cursor: Option<String>) -> Result<MyReportsPage, ServerFnError> {
    use axum::extract::State;
    use leptos_axum::extract;

    let State(state): State<crate::app::AppState> = extract().await?;

    let mut url = format!("{}/api/v1/stolen/reports?limit=25", state.api_base_url);
    if let Some(c) = cursor {
        url.push_str(&format!("&cursor={c}"));
    }

    let resp = state
        .http_client
        .get(&url)
        .send()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    let page: MyReportsPage = resp
        .json()
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))?;

    Ok(page)
}

// ─── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn MyReports() -> impl IntoView {
    let reports = Resource::new(|| (), |_| fetch_my_reports(None));

    view! {
        <div class="max-w-4xl mx-auto p-6">
            <div class="flex items-center justify-between mb-6">
                <h1 class="text-2xl font-bold text-gray-900">"My Reports"</h1>
                <A
                    href="/stolen/report"
                    attr:class="bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium px-4 py-2 rounded-md"
                >
                    "+ Report a stolen card"
                </A>
            </div>

            <Suspense fallback=move || view! { <LoadingState /> }>
                {move || {
                    reports.get().map(|result| match result {
                        Err(e) => view! { <ErrorState message=e.to_string() /> }.into_any(),
                        Ok(page) if page.data.is_empty() => {
                            view! { <EmptyState /> }.into_any()
                        }
                        Ok(page) => {
                            view! { <ReportList reports=page.data /> }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn ReportList(reports: Vec<ReportSummary>) -> impl IntoView {
    view! {
        <div class="space-y-3">
            <For
                each=move || reports.clone()
                key=|r| r.id.clone()
                children=|report| view! { <ReportRow report /> }
            />
        </div>
    }
}

#[component]
fn ReportRow(report: ReportSummary) -> impl IntoView {
    let (badge_class, badge_text) = status_badge(&report.status);

    view! {
        <div class="border border-gray-200 rounded-lg p-4 bg-white shadow-sm">
            <div class="flex items-start justify-between gap-4">
                <div>
                    <div class="flex items-center gap-2 mb-1">
                        <span class="font-mono text-sm font-medium text-gray-900">
                            {format!("{} — {}", report.grader, report.cert_number)}
                        </span>
                        <span class=format!("inline-flex items-center px-2 py-0.5 rounded text-xs font-medium {badge_class}")>
                            {badge_text}
                        </span>
                        {if report.scope == "private" {
                            Some(view! {
                                <span class="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-700">
                                    "Private"
                                </span>
                            })
                        } else {
                            None
                        }}
                    </div>

                    // Contextual guidance per status
                    <StatusGuidance status=report.status.clone() />

                    {report.evidence.as_ref().map(|e| view! {
                        <p class="text-sm text-gray-600 mt-1 line-clamp-2">{e.clone()}</p>
                    })}
                </div>

                <div class="text-right text-xs text-gray-400 whitespace-nowrap shrink-0">
                    <div>{report.created_at.clone()}</div>
                    {report.confirmed_at.as_ref().map(|dt| view! {
                        <div>"Confirmed: " {dt.clone()}</div>
                    })}
                </div>
            </div>
        </div>
    }
}

#[component]
fn StatusGuidance(status: String) -> impl IntoView {
    let (text, class) = match status.as_str() {
        "pending" => (
            "Awaiting moderator review.  You will be notified of any status change.",
            "text-amber-700 bg-amber-50",
        ),
        "confirmed" => (
            "Confirmed — this cert is now on the community stolen list.",
            "text-green-700 bg-green-50",
        ),
        "disputed" => (
            "Disputed by another user.  A moderator will adjudicate.",
            "text-orange-700 bg-orange-50",
        ),
        "rejected" => (
            "Rejected by a moderator.  If you believe this is in error, contact support.",
            "text-red-700 bg-red-50",
        ),
        "resolved" => (
            "Resolved — the card has been recovered and removed from active matching.",
            "text-gray-700 bg-gray-50",
        ),
        _ => ("", ""),
    };

    if text.is_empty() {
        return view! { <span /> }.into_any();
    }

    view! {
        <p class=format!("text-xs rounded px-2 py-1 mt-1 {class}")>{text}</p>
    }
    .into_any()
}

// ─── Empty / loading / error states ──────────────────────────────────────────

#[component]
fn EmptyState() -> impl IntoView {
    view! {
        <div class="text-center py-16 text-gray-500">
            <svg class="mx-auto mb-4 h-12 w-12 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
            </svg>
            <p class="text-lg font-medium text-gray-900 mb-1">"No reports yet"</p>
            <p class="text-sm text-gray-500 mb-4">
                "If you know of a stolen graded card, report it to protect the community."
            </p>
            <A
                href="/stolen/report"
                attr:class="bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium px-4 py-2 rounded-md"
            >
                "Report a stolen card"
            </A>
        </div>
    }
}

#[component]
fn LoadingState() -> impl IntoView {
    view! {
        <div class="space-y-3">
            {(0..3).map(|_| view! {
                <div class="border border-gray-200 rounded-lg p-4 bg-white animate-pulse">
                    <div class="h-4 bg-gray-200 rounded w-1/3 mb-2" />
                    <div class="h-3 bg-gray-100 rounded w-2/3" />
                </div>
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
fn ErrorState(message: String) -> impl IntoView {
    view! {
        <div class="rounded-md bg-red-50 border border-red-200 p-4">
            <p class="text-red-800 font-medium">"Failed to load reports"</p>
            <p class="text-red-700 text-sm mt-1">{message}</p>
        </div>
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn status_badge(status: &str) -> (&'static str, &'static str) {
    match status {
        "pending"   => ("bg-amber-100 text-amber-800",  "Pending"),
        "confirmed" => ("bg-green-100 text-green-800",  "Confirmed"),
        "disputed"  => ("bg-orange-100 text-orange-800","Disputed"),
        "rejected"  => ("bg-red-100 text-red-800",      "Rejected"),
        "resolved"  => ("bg-gray-100 text-gray-700",    "Resolved"),
        _           => ("bg-gray-100 text-gray-700",    "Unknown"),
    }
}
