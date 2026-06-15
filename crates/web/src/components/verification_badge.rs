use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum VerificationStatusDisplay {
    Verified,
    Unverified,
    Mismatch,
}

/// Badge showing a card instance's grading-cert verification status.
#[component]
pub fn VerificationBadge(status: VerificationStatusDisplay) -> impl IntoView {
    let (label, classes) = match status {
        VerificationStatusDisplay::Verified => {
            ("Verified", "bg-green-100 text-green-800 border-green-200")
        }
        VerificationStatusDisplay::Unverified => (
            "Unverified",
            "bg-yellow-100 text-yellow-800 border-yellow-200",
        ),
        VerificationStatusDisplay::Mismatch => {
            ("Mismatch", "bg-red-100 text-red-800 border-red-200")
        }
    };

    view! {
        <span class=format!(
            "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium {classes}"
        )>
            {label}
        </span>
    }
}
