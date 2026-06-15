use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::use_params_map,
    path,
};
use uuid::Uuid;

use crate::routes::{
    auth::{
        login::LoginPage,
        onboarding::OnboardingPage,
        register::RegisterPage,
        reset_password::{ConfirmResetPage, RequestResetPage},
        session::{provide_auth, AccessToken, Session},
        verify_email::VerifyEmailPage,
    },
    catalogue::add::AddCardPage,
    import::{ExportPage, ImportWizard},
    inventory::{InventoryDetailPage, InventoryListPage},
    reconcile::{MappingQueuePage, ReconcileDashboard, ReconcileReportPage},
    risk::RiskDashboardPage,
    settings::{pos::PosSettingsPage, team::TeamPage},
    stolen::{DisputePage, ModeratorQueue, MyReports, ReportForm},
    value::{CardHistoryPage, ValuePage},
    verify::VerifyPage,
};

#[component]
fn ImportPage() -> impl IntoView {
    let params = use_params_map();
    let wid = params.with(|p| {
        p.get("wid")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
    });
    wid.map(|id| view! { <ImportWizard workspace_id=id /> })
}

#[component]
fn ExportRoute() -> impl IntoView {
    let params = use_params_map();
    let wid = params.with(|p| {
        p.get("wid")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
    });
    wid.map(|id| view! { <ExportPage workspace_id=id /> })
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Provide auth context at the root so every route can access it.
    let session = Session::new();
    let access_token = AccessToken::new();
    provide_auth(session, access_token);

    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Page not found."</p> }>
                // Auth routes
                <Route path=path!("/auth/login") view=LoginPage />
                <Route path=path!("/auth/register") view=RegisterPage />
                <Route path=path!("/auth/reset-password") view=RequestResetPage />
                <Route path=path!("/auth/reset-password/confirm") view=ConfirmResetPage />
                <Route path=path!("/auth/verify-email") view=VerifyEmailPage />
                <Route path=path!("/auth/onboarding") view=OnboardingPage />

                <Route path=path!("/workspaces/:wid/reconcile") view=ReconcileDashboard />
                <Route path=path!("/workspaces/:wid/reconcile/report/:rid") view=ReconcileReportPage />
                <Route path=path!("/workspaces/:wid/reconcile/mapping") view=MappingQueuePage />
                <Route path=path!("/settings/:wid/team") view=TeamPage />
                <Route path=path!("/settings/:wid/pos") view=PosSettingsPage />
                <Route path=path!("/stolen/report") view=ReportForm />
                <Route path=path!("/stolen/my-reports") view=MyReports />
                <Route path=path!("/stolen/queue") view=ModeratorQueue />
                <Route path=path!("/stolen/dispute/:id") view=DisputePage />
                <Route path=path!("/catalogue/:wid/import") view=ImportPage />
                <Route path=path!("/catalogue/:wid/export") view=ExportRoute />
                <Route path=path!("/workspaces/:wid/risk") view=RiskDashboardPage />
                <Route path=path!("/catalogue/add") view=AddCardPage />
                <Route path=path!("/verify") view=VerifyPage />
                <Route path=path!("/workspaces/:wid/inventory") view=InventoryListPage />
                <Route path=path!("/workspaces/:wid/inventory/:id") view=InventoryDetailPage />
                <Route path=path!("/value/:wid") view=ValuePage />
                <Route path=path!("/value/:wid/card/:printing_id/history") view=CardHistoryPage />
            </Routes>
        </Router>
    }
}
