use leptos::prelude::*;

/// A single-value stat card used in dashboards (e.g. total collection value).
#[component]
pub fn StatWidget(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(optional, into)] sub_label: Option<String>,
) -> impl IntoView {
    view! {
        <div class="rounded-lg border border-gray-200 bg-white px-5 py-4 shadow-sm">
            <p class="text-xs font-medium text-gray-500 uppercase tracking-wide">{label}</p>
            <p class="mt-1 text-2xl font-bold text-gray-900">{value}</p>
            {sub_label.map(|s| view! {
                <p class="mt-0.5 text-xs text-gray-400">{s}</p>
            })}
        </div>
    }
}
