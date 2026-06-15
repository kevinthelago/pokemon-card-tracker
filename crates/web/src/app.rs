use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::routes::{
    reconcile::{MappingQueuePage, ReconcileDashboard, ReconcileReportPage},
    settings::team::TeamPage,
};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Page not found."</p> }>
                <Route path=path!("/workspaces/:wid/reconcile") view=ReconcileDashboard />
                <Route path=path!("/workspaces/:wid/reconcile/report/:rid") view=ReconcileReportPage />
                <Route path=path!("/workspaces/:wid/reconcile/mapping") view=MappingQueuePage />
                <Route path=path!("/settings/:wid/team") view=TeamPage />
            </Routes>
        </Router>
    }
}
