use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::use_params_map,
    path,
};
use uuid::Uuid;

use crate::routes::{
    import::{ExportPage, ImportWizard},
    settings::team::TeamPage,
    stolen::{DisputePage, ModeratorQueue, MyReports, ReportForm},
};

#[component]
fn ImportPage() -> impl IntoView {
    let params = use_params_map();
    let wid = params.with(|p| p.get("wid").as_deref().and_then(|s| Uuid::parse_str(s).ok()));
    wid.map(|id| view! { <ImportWizard workspace_id=id /> })
}

#[component]
fn ExportRoute() -> impl IntoView {
    let params = use_params_map();
    let wid = params.with(|p| p.get("wid").as_deref().and_then(|s| Uuid::parse_str(s).ok()));
    wid.map(|id| view! { <ExportPage workspace_id=id /> })
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Page not found."</p> }>
                <Route path=path!("/settings/:wid/team") view=TeamPage />
                <Route path=path!("/stolen/report") view=ReportForm />
                <Route path=path!("/stolen/my-reports") view=MyReports />
                <Route path=path!("/stolen/queue") view=ModeratorQueue />
                <Route path=path!("/stolen/dispute/:id") view=DisputePage />
                <Route path=path!("/catalogue/:wid/import") view=ImportPage />
                <Route path=path!("/catalogue/:wid/export") view=ExportRoute />
            </Routes>
        </Router>
    }
}
