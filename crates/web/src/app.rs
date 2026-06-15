use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::routes::settings::team::TeamPage;
use crate::routes::stolen::{DisputePage, ModeratorQueue, MyReports, ReportForm};

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
            </Routes>
        </Router>
    }
}
