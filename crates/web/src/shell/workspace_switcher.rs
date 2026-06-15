use leptos::prelude::*;

/// Workspace switcher — shows the active workspace name and allows switching.
/// The active workspace ID is stored in a context signal set during auth.
#[component]
pub fn WorkspaceSwitcher() -> impl IntoView {
    // TODO (auth stream): wire to the active workspace context signal
    let workspace_name = move || "My Workspace".to_string();

    view! {
        <div class="relative">
            <button
                class="flex items-center gap-1 text-sm font-medium text-gray-700 hover:text-gray-900 \
                       border border-gray-300 rounded-md px-3 py-1.5 bg-white hover:bg-gray-50"
                aria-label="Switch workspace"
            >
                <span class="max-w-[140px] truncate">{workspace_name}</span>
                <ChevronDownIcon />
            </button>
            // TODO (auth stream): dropdown menu with workspace list
        </div>
    }
}

#[component]
fn ChevronDownIcon() -> impl IntoView {
    view! {
        <svg class="h-3.5 w-3.5 text-gray-500" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd"
                d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 \
                   1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z"
                clip-rule="evenodd" />
        </svg>
    }
}
