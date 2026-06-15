use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::routes::import::{ImportWizardPage, ExportPage};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Router>
            <main>
                <Routes fallback=|| "Page not found">
                    <Route path=path!("/import") view=ImportWizardPage />
                    <Route path=path!("/export") view=ExportPage />
                    <Route path=path!("/") view=Home />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Home() -> impl IntoView {
    view! {
        <div class="p-8">
            <h1 class="text-2xl font-bold mb-4">"CardGuard"</h1>
            <ul class="space-y-2">
                <li><a href="/import" class="text-blue-600 underline">"Import cards from CSV"</a></li>
                <li><a href="/export" class="text-blue-600 underline">"Export catalogue to CSV"</a></li>
            </ul>
        </div>
    }
}

