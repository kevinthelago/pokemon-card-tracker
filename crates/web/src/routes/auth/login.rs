use leptos::prelude::*;
use leptos_router::components::A;

use super::session::{api_login, use_access_token, use_session, SessionUser};

#[component]
pub fn LoginPage() -> impl IntoView {
    let session = use_session();
    let access_token = use_access_token();

    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();

        let email_val = email.get();
        let password_val = password.get();

        if email_val.is_empty() || password_val.is_empty() {
            set_error.set(Some("Email and password are required.".into()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match api_login(&email_val, &password_val).await {
                Ok(resp) => {
                    access_token.set(resp.access_token);
                    session.set_user(SessionUser {
                        user_id: resp.user_id,
                        email: resp.email,
                        name: resp.name,
                        workspace_id: resp.workspace_id,
                    });
                    // Navigate to inventory
                    let window = web_sys::window().unwrap();
                    let _ = window.location().set_href("/inventory");
                }
                Err(msg) => {
                    set_error.set(Some(msg));
                    set_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="min-h-screen flex items-center justify-center bg-gray-50">
            <div class="w-full max-w-md">
                <div class="bg-white shadow-sm rounded-xl px-8 py-10 space-y-6">
                    <div class="text-center">
                        <h1 class="text-2xl font-semibold text-gray-900">"Sign in to CardGuard"</h1>
                    </div>

                    {move || error.get().map(|msg| view! {
                        <div class="rounded-md bg-red-50 border border-red-200 px-4 py-3">
                            <p class="text-sm text-red-700">{msg}</p>
                        </div>
                    })}

                    <form on:submit=on_submit class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1" for="email">
                                "Email"
                            </label>
                            <input
                                id="email"
                                type="email"
                                autocomplete="email"
                                required
                                class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                prop:value=move || email.get()
                                on:input=move |ev| set_email.set(event_target_value(&ev))
                            />
                        </div>

                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1" for="password">
                                "Password"
                            </label>
                            <input
                                id="password"
                                type="password"
                                autocomplete="current-password"
                                required
                                class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                prop:value=move || password.get()
                                on:input=move |ev| set_password.set(event_target_value(&ev))
                            />
                        </div>

                        <div class="flex items-center justify-end">
                            <A href="/auth/reset-password" class="text-sm text-indigo-600 hover:underline">
                                "Forgot password?"
                            </A>
                        </div>

                        <button
                            type="submit"
                            disabled=move || loading.get()
                            class="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                            {move || if loading.get() { "Signing in\u{2026}" } else { "Sign in" }}
                        </button>
                    </form>

                    <p class="text-center text-sm text-gray-500">
                        "Don\u{2019}t have an account? "
                        <A href="/auth/register" class="text-indigo-600 hover:underline">"Sign up"</A>
                    </p>
                </div>
            </div>
        </div>
    }
}
