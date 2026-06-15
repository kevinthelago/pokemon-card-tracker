use super::WorkspaceSwitcher;
use leptos::prelude::*;

/// Top navigation bar — visible on all authenticated pages.
#[component]
pub fn NavBar() -> impl IntoView {
    view! {
        <nav class="bg-white border-b border-gray-200 shadow-sm">
            <div class="container mx-auto px-4 flex items-center justify-between h-14">
                <div class="flex items-center gap-6">
                    <a href="/" class="font-bold text-indigo-600 text-lg">
                        "CardGuard"
                    </a>
                    <NavLink href="/inventory" label="Inventory" />
                    <NavLink href="/risk"      label="Risk" />
                    <NavLink href="/pos"       label="POS" />
                </div>

                <div class="flex items-center gap-4">
                    <WorkspaceSwitcher />
                    <a href="/settings" class="text-sm text-gray-600 hover:text-gray-900">
                        "Settings"
                    </a>
                </div>
            </div>
        </nav>
    }
}

#[component]
fn NavLink(href: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <a
            href=href
            class="text-sm text-gray-600 hover:text-indigo-600 font-medium transition-colors"
        >
            {label}
        </a>
    }
}
