use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::routes::{
    auth::{
        login::LoginPage,
        onboarding::OnboardingPage,
        register::RegisterPage,
        reset_password::{ConfirmResetPage, RequestResetPage},
        session::{provide_auth, AccessToken, Session},
        verify_email::VerifyEmailPage,
    },
    settings::team::TeamPage,
};

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

                // Team settings
                <Route path=path!("/settings/:wid/team") view=TeamPage />
            </Routes>
        </Router>
    }
}
