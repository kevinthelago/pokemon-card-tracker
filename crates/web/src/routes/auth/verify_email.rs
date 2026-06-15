use gloo_net::http::Request;
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_query_map};

const API: &str = "/api";

async fn api_verify_email(token: String) -> Result<(), String> {
    let resp = Request::get(&format!("{API}/auth/verify-email"))
        .query([("token", token.as_str())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Verification failed")
            .to_owned());
    }
    Ok(())
}

/// Picks `?token=` from the URL, fires the verification request automatically,
/// and shows success or error state.
#[component]
pub fn VerifyEmailPage() -> impl IntoView {
    let query = use_query_map();
    let token = move || query.with(|q| q.get("token").unwrap_or_default());

    let status = LocalResource::new(move || {
        let tok = token();
        async move {
            if tok.is_empty() {
                return Err("No verification token in URL.".to_owned());
            }
            api_verify_email(tok).await
        }
    });

    view! {
        <div class="min-h-screen flex items-center justify-center bg-gray-50">
            <div class="w-full max-w-md">
                <div class="bg-white shadow-sm rounded-xl px-8 py-10">
                    <Suspense fallback=move || view! {
                        <div class="text-center text-gray-500 py-8">"Verifying your email\u{2026}"</div>
                    }>
                        {move || status.get().as_deref().map(|result| match result {
                            Ok(()) => view! {
                                <div class="text-center space-y-4">
                                    <div class="text-green-600 text-5xl">"✓"</div>
                                    <h1 class="text-xl font-semibold text-gray-900">"Email verified"</h1>
                                    <p class="text-sm text-gray-500">"Your email address has been confirmed."</p>
                                    <A
                                        href="/inventory"
                                        attr:class="inline-block rounded-lg bg-indigo-600 px-5 py-2 text-sm font-semibold text-white hover:bg-indigo-700"
                                    >
                                        "Go to catalogue"
                                    </A>
                                </div>
                            }.into_any(),
                            Err(msg) => {
                                let msg = msg.clone();
                                view! {
                                    <div class="text-center space-y-4">
                                        <div class="text-red-500 text-5xl">"✗"</div>
                                        <h1 class="text-xl font-semibold text-gray-900">"Verification failed"</h1>
                                        <p class="text-sm text-gray-500">{msg}</p>
                                        <A
                                            href="/auth/login"
                                            attr:class="inline-block text-sm text-indigo-600 hover:underline"
                                        >
                                            "Back to sign in"
                                        </A>
                                    </div>
                                }.into_any()
                            },
                        })}
                    </Suspense>
                </div>
            </div>
        </div>
    }
}
