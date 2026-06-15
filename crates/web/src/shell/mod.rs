mod nav;
mod workspace_switcher;

pub use nav::NavBar;
pub use workspace_switcher::WorkspaceSwitcher;

use leptos::prelude::*;
use leptos_router::components::Outlet;

/// Authenticated app shell — navigation + workspace switcher + outlet for child routes.
#[component]
pub fn AppShell() -> impl IntoView {
    view! {
        <div class="min-h-screen flex flex-col">
            <NavBar />
            <main class="flex-1 container mx-auto px-4 py-6">
                <Outlet />
            </main>
        </div>
    }
}
