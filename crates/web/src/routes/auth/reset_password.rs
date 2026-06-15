use gloo_net::http::Request;
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_query_map};
use wasm_bindgen_futures::spawn_local;

const API: &str = "/api";

async fn api_request_reset(email: &str) -> Result<(), String> {
    let resp = Request::post(&format!("{API}/auth/password-reset"))
        .json(&serde_json::json!({ "email": email }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err("Request failed. Please try again.".into());
    }
    Ok(())
}

async fn api_confirm_reset(token: &str, new_password: &str) -> Result<(), String> {
    let resp = Request::post(&format!("{API}/auth/password-reset/confirm"))
        .json(&serde_json::json!({ "token": token, "new_password": new_password }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Reset failed")
            .to_owned());
    }
    Ok(())
}

// ─── Request reset (enter email) ─────────────────────────────────────────────

#[component]
pub fn RequestResetPage() -> impl IntoView {
    let (email, set_email) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (sent, set_sent) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let val = email.get();
        if val.is_empty() {
            set_error.set(Some("Please enter your email address.".into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api_request_reset(&val).await {
                Ok(()) => set_sent.set(true),
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
                <div class="bg-white shadow-sm rounded-xl px-8 py-10">
                    {move || if sent.get() {
                        view! {
                            <div class="text-center space-y-3">
                                <div class="text-green-600 text-4xl">"✓"</div>
                                <h1 class="text-xl font-semibold text-gray-900">"Check your email"</h1>
                                <p class="text-sm text-gray-500">
                                    "If an account exists for that address we\u{2019}ve sent a reset link."
                                </p>
                                <A href="/auth/login" attr:class="inline-block text-sm text-indigo-600 hover:underline">
                                    "Back to sign in"
                                </A>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="space-y-6">
                                <div class="text-center">
                                    <h1 class="text-2xl font-semibold text-gray-900">"Reset your password"</h1>
                                    <p class="mt-1 text-sm text-gray-500">"Enter your email and we\u{2019}ll send a reset link."</p>
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
                                            required
                                            class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                            prop:value=move || email.get()
                                            on:input=move |ev| set_email.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <button
                                        type="submit"
                                        disabled=move || loading.get()
                                        class="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-50"
                                    >
                                        {move || if loading.get() { "Sending\u{2026}" } else { "Send reset link" }}
                                    </button>
                                </form>

                                <p class="text-center text-sm text-gray-500">
                                    <A href="/auth/login" attr:class="text-indigo-600 hover:underline">"Back to sign in"</A>
                                </p>
                            </div>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

// ─── Confirm reset (token from URL) ──────────────────────────────────────────

#[component]
pub fn ConfirmResetPage() -> impl IntoView {
    let query = use_query_map();
    let token = move || query.with(|q| q.get("token").unwrap_or_default());

    let (password, set_password) = signal(String::new());
    let (confirm, set_confirm) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (done, set_done) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let tok = token();
        let pw = password.get();
        let cf = confirm.get();

        if tok.is_empty() {
            set_error.set(Some("Invalid or missing reset token.".into()));
            return;
        }
        if pw != cf {
            set_error.set(Some("Passwords do not match.".into()));
            return;
        }
        if pw.len() < 8 {
            set_error.set(Some("Password must be at least 8 characters.".into()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match api_confirm_reset(&tok, &pw).await {
                Ok(()) => set_done.set(true),
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
                <div class="bg-white shadow-sm rounded-xl px-8 py-10">
                    {move || if done.get() {
                        view! {
                            <div class="text-center space-y-3">
                                <div class="text-green-600 text-4xl">"✓"</div>
                                <h1 class="text-xl font-semibold text-gray-900">"Password updated"</h1>
                                <p class="text-sm text-gray-500">"Your password has been changed."</p>
                                <A href="/auth/login" attr:class="inline-block text-sm font-medium text-indigo-600 hover:underline">
                                    "Sign in with new password"
                                </A>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="space-y-6">
                                <div class="text-center">
                                    <h1 class="text-2xl font-semibold text-gray-900">"Set new password"</h1>
                                </div>

                                {move || error.get().map(|msg| view! {
                                    <div class="rounded-md bg-red-50 border border-red-200 px-4 py-3">
                                        <p class="text-sm text-red-700">{msg}</p>
                                    </div>
                                })}

                                <form on:submit=on_submit class="space-y-4">
                                    <div>
                                        <label class="block text-sm font-medium text-gray-700 mb-1" for="pw">
                                            "New password"
                                        </label>
                                        <input
                                            id="pw"
                                            type="password"
                                            autocomplete="new-password"
                                            required
                                            class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm"
                                            prop:value=move || password.get()
                                            on:input=move |ev| set_password.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div>
                                        <label class="block text-sm font-medium text-gray-700 mb-1" for="cf">
                                            "Confirm new password"
                                        </label>
                                        <input
                                            id="cf"
                                            type="password"
                                            autocomplete="new-password"
                                            required
                                            class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm"
                                            prop:value=move || confirm.get()
                                            on:input=move |ev| set_confirm.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <button
                                        type="submit"
                                        disabled=move || loading.get()
                                        class="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-50"
                                    >
                                        {move || if loading.get() { "Updating\u{2026}" } else { "Update password" }}
                                    </button>
                                </form>
                            </div>
                        }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}
