use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum RiskSeverityDisplay {
    Low,
    Medium,
    High,
    Critical,
}

/// Compact chip showing a risk flag's severity.
#[component]
pub fn RiskChip(
    severity: RiskSeverityDisplay,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let (default_label, classes) = match severity {
        RiskSeverityDisplay::Low => ("Low", "bg-gray-100 text-gray-700 border-gray-200"),
        RiskSeverityDisplay::Medium => {
            ("Medium", "bg-yellow-100 text-yellow-800 border-yellow-200")
        }
        RiskSeverityDisplay::High => ("High", "bg-orange-100 text-orange-800 border-orange-200"),
        RiskSeverityDisplay::Critical => ("Critical", "bg-red-100 text-red-800 border-red-200"),
    };

    let text = label.unwrap_or_else(|| default_label.to_string());

    view! {
        <span class=format!(
            "inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-semibold {classes}"
        )>
            {text}
        </span>
    }
}
