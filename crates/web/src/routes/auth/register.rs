use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen_futures::spawn_local;

use super::session::{api_register, use_access_token, use_session, SessionUser};

#[component]
pub fn RegisterPage() -> impl IntoView {
    let session = use_session();
    let access_token = use_access_token();

    let (email, set_email) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (confirm, set_confirm) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();

        let email_val = email.get();
        let name_val = name.get();
        let password_val = password.get();
        let confirm_val = confirm.get();

        if email_val.is_empty() || name_val.is_empty() || password_val.is_empty() {
            set_error.set(Some("All fields are required.".into()));
            return;
        }
        if password_val != confirm_val {
            set_error.set(Some("Passwords do not match.".into()));
            return;
        }
        if password_val.len() < 8 {
            set_error.set(Some("Password must be at least 8 characters.".into()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match api_register(&email_val, &name_val, &password_val).await {
                Ok(resp) => {
                    access_token.set(resp.access_token);
                    session.set_user(SessionUser {
                        user_id: resp.user_id,
                        email: resp.email,
                        name: resp.name,
                        workspace_id: resp.workspace_id,
                    });
                    let window = web_sys::window().unwrap();
                    let _ = window.location().set_href("/auth/onboarding");
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
                        <h1 class="text-2xl font-semibold text-gray-900">"Create your account"</h1>
                        <p class="mt-1 text-sm text-gray-500">"Free to start. No credit card required."</p>
                    </div>

                    {move || error.get().map(|msg| view! {
                        <div class="rounded-md bg-red-50 border border-red-200 px-4 py-3">
                            <p class="text-sm text-red-700">{msg}</p>
                        </div>
                    })}

                    <form on:submit=on_submit class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1" for="name">
                                "Full name"
                            </label>
                            <input
                                id="name"
                                type="text"
                                autocomplete="name"
                                required
                                class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                prop:value=move || name.get()
                                on:input=move |ev| set_name.set(event_target_value(&ev))
                            />
                        </div>

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
                                autocomplete="new-password"
                                required
                                class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                prop:value=move || password.get()
                                on:input=move |ev| set_password.set(event_target_value(&ev))
                            />
                            <p class="mt-1 text-xs text-gray-400">"Minimum 8 characters."</p>
                        </div>

                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1" for="confirm">
                                "Confirm password"
                            </label>
                            <input
                                id="confirm"
                                type="password"
                                autocomplete="new-password"
                                required
                                class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                prop:value=move || confirm.get()
                                on:input=move |ev| set_confirm.set(event_target_value(&ev))
                            />
                        </div>

                        <button
                            type="submit"
                            disabled=move || loading.get()
                            class="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                            {move || if loading.get() { "Creating account\u{2026}" } else { "Create account" }}
                        </button>
                    </form>

                    <p class="text-center text-sm text-gray-500">
                        "Already have an account? "
                        <A href="/auth/login" attr:class="text-indigo-600 hover:underline">"Sign in"</A>
                    </p>
                </div>
            </div>
        </div>
    }
}
