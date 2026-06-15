use leptos::*;
use leptos_meta::*;
use leptos_router::*;

use crate::routes::reconcile::{
    mapping::MappingQueuePage, mod_route::ReconcileDashboard, report::ReconcileReportPage,
};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/cardguard.css"/>
        <Title text="CardGuard"/>

        <Router>
            <nav class="nav">
                <a href="/reconcile">"Reconcile"</a>
                <a href="/reconcile/mapping">"SKU Mapping"</a>
            </nav>
            <main>
                <Routes>
                    <Route path="/reconcile" view=ReconcileDashboard/>
                    <Route path="/reconcile/report/:report_id" view=ReconcileReportPage/>
                    <Route path="/reconcile/mapping" view=MappingQueuePage/>
                    <Route path="/" view=|| view! { <p>"Welcome to CardGuard"</p> }/>
                </Routes>
            </main>
        </Router>
    }
}
