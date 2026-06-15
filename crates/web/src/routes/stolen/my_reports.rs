//! "My Reports" screen — stolen-card reports submitted by the current user (CSR).

use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

use crate::api;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub id: String,
    pub grader: String,
    pub cert_number: String,
    pub status: String,
    pub evidence: Option<String>,
    pub confirmed_at: Option<String>,
    pub created_at: String,
}

#[component]
pub fn MyReports() -> impl IntoView {
    let (reload, set_reload) = signal(0u32);

    let reports = LocalResource::new(move || {
        let _ = reload.get();
        async move { api::fetch_my_reports().await }
    });

    let _trigger_reload = move || set_reload.update(|n| *n += 1);

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
                    reports.get().as_deref().map(|result| match result {
                        Err(e) => view! { <ErrorState message=e.clone() /> }.into_any(),
                        Ok(items) if items.is_empty() => view! { <EmptyState /> }.into_any(),
                        Ok(items) => {
                            let items = items.clone();
                            view! {
                                <div class="space-y-3">
                                    <For
                                        each=move || items.clone()
                                        key=|r| r.id.clone()
                                        children=|r| view! { <ReportRow report=r /> }
                                    />
                                </div>
                            }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn ReportRow(report: ReportSummary) -> impl IntoView {
    let (badge_class, badge_text) = status_badge(&report.status);

    view! {
        <div class="border border-gray-200 rounded-lg p-4 bg-white shadow-sm">
            <div class="flex items-start justify-between gap-4">
                <div class="flex-1">
                    <div class="flex items-center gap-2 mb-1">
                        <span class="font-mono text-sm font-medium text-gray-900">
                            {format!("{} — {}", report.grader, report.cert_number)}
                        </span>
                        <span class=format!("inline-flex items-center px-2 py-0.5 rounded text-xs font-medium {badge_class}")>
                            {badge_text}
                        </span>
                    </div>
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
            "Awaiting moderator review. You will be notified of any status change.",
            "text-amber-700 bg-amber-50",
        ),
        "confirmed" => (
            "Confirmed — this cert is now on the community stolen list.",
            "text-green-700 bg-green-50",
        ),
        "disputed" => (
            "Disputed by another user. A moderator will adjudicate.",
            "text-orange-700 bg-orange-50",
        ),
        "rejected" => (
            "Rejected by a moderator. If you believe this is in error, contact support.",
            "text-red-700 bg-red-50",
        ),
        _ => return view! { <span /> }.into_any(),
    };

    view! {
        <p class=format!("text-xs rounded px-2 py-1 mt-1 {class}")>{text}</p>
    }
    .into_any()
}

#[component]
fn EmptyState() -> impl IntoView {
    view! {
        <div class="text-center py-16">
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
            }).collect_view()}
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

fn status_badge(status: &str) -> (&'static str, &'static str) {
    match status {
        "pending" => ("bg-amber-100 text-amber-800", "Pending"),
        "confirmed" => ("bg-green-100 text-green-800", "Confirmed"),
        "disputed" => ("bg-orange-100 text-orange-800", "Disputed"),
        "rejected" => ("bg-red-100 text-red-800", "Rejected"),
        _ => ("bg-gray-100 text-gray-700", "Unknown"),
    }
}
