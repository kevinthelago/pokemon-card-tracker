use gloo_net::http::Request;
use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;

use super::session::{use_access_token, use_session};

const API: &str = "/api";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkspaceDto {
    id: Uuid,
    name: String,
    kind: String,
}

async fn api_create_workspace(name: &str, kind: &str, token: &str) -> Result<WorkspaceDto, String> {
    let resp = Request::post(&format!("{API}/workspaces"))
        .header("Authorization", &format!("Bearer {token}"))
        .json(&serde_json::json!({ "name": name, "kind": kind }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Failed to create workspace")
            .to_owned());
    }
    resp.json::<WorkspaceDto>().await.map_err(|e| e.to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Collector,
    Seller,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Collector => "collector",
            Kind::Seller => "seller",
        }
    }
}

/// First-run onboarding: choose workspace type and set a name.
/// Shown after registration; also reachable if the user has no workspaces.
#[component]
pub fn OnboardingPage() -> impl IntoView {
    let session = use_session();
    let access_token = use_access_token();

    let (kind, set_kind) = signal(Kind::Collector);
    let (name, set_name) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    // Derive default name from kind + session
    let default_name = move || {
        let base = session
            .get()
            .map(|u| {
                let local = u.email.split('@').next().unwrap_or("user");
                local.to_owned()
            })
            .unwrap_or_else(|| "user".into());
        match kind.get() {
            Kind::Collector => format!("{base}\u{2019}s Collection"),
            Kind::Seller => "My Shop".into(),
        }
    };

    // Keep name in sync with kind changes until the user edits it
    let (name_edited, set_name_edited) = signal(false);
    let _ = Effect::new(move |_| {
        if !name_edited.get() {
            set_name.set(default_name());
        }
    });

    let on_select_kind = move |k: Kind| {
        set_kind.set(k);
        if !name_edited.get() {
            set_name.set(default_name());
        }
    };

    let on_submit = move |ev: web_sys::SubmitEvent| {
        ev.prevent_default();
        let n = name.get();
        let k = kind.get().as_str().to_owned();
        let tok = access_token.get().unwrap_or_default();

        if n.trim().is_empty() {
            set_error.set(Some("Please enter a workspace name.".into()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        spawn_local(async move {
            match api_create_workspace(&n, &k, &tok).await {
                Ok(ws) => {
                    if let Some(mut user) = session.get() {
                        user.workspace_id = ws.id;
                        session.set_user(user);
                    }
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
            <div class="w-full max-w-lg">
                <div class="bg-white shadow-sm rounded-xl px-8 py-10 space-y-8">
                    <div class="text-center">
                        <h1 class="text-2xl font-semibold text-gray-900">"Set up your workspace"</h1>
                        <p class="mt-1 text-sm text-gray-500">"How will you use CardGuard?"</p>
                    </div>

                    {move || error.get().map(|msg| view! {
                        <div class="rounded-md bg-red-50 border border-red-200 px-4 py-3">
                            <p class="text-sm text-red-700">{msg}</p>
                        </div>
                    })}

                    <div class="grid grid-cols-2 gap-4">
                        <button
                            type="button"
                            class=move || {
                                let base = "border-2 rounded-xl p-5 text-left transition-colors";
                                if kind.get() == Kind::Collector {
                                    format!("{base} border-indigo-500 bg-indigo-50")
                                } else {
                                    format!("{base} border-gray-200 hover:border-indigo-300")
                                }
                            }
                            on:click=move |_| on_select_kind(Kind::Collector)
                        >
                            <div class="text-2xl mb-2">"📦"</div>
                            <div class="font-semibold text-gray-900">"Personal collector"</div>
                            <div class="text-xs text-gray-500 mt-1">"Track and value your collection."</div>
                        </button>

                        <button
                            type="button"
                            class=move || {
                                let base = "border-2 rounded-xl p-5 text-left transition-colors";
                                if kind.get() == Kind::Seller {
                                    format!("{base} border-indigo-500 bg-indigo-50")
                                } else {
                                    format!("{base} border-gray-200 hover:border-indigo-300")
                                }
                            }
                            on:click=move |_| on_select_kind(Kind::Seller)
                        >
                            <div class="text-2xl mb-2">"🏪"</div>
                            <div class="font-semibold text-gray-900">"Seller / shop"</div>
                            <div class="text-xs text-gray-500 mt-1">"POS sync, fraud detection, team access."</div>
                        </button>
                    </div>

                    <form on:submit=on_submit class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1" for="ws-name">
                                "Workspace name"
                            </label>
                            <input
                                id="ws-name"
                                type="text"
                                maxlength="100"
                                required
                                class="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
                                prop:value=move || name.get()
                                on:input=move |ev| {
                                    set_name_edited.set(true);
                                    set_name.set(event_target_value(&ev));
                                }
                            />
                        </div>

                        <button
                            type="submit"
                            disabled=move || loading.get()
                            class="w-full rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                            {move || if loading.get() { "Creating\u{2026}" } else { "Get started" }}
                        </button>
                    </form>
                </div>
            </div>
        </div>
    }
}

// ─── Workspace switcher (used from app shell) ─────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceListItem {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
}

async fn api_list_workspaces(token: &str) -> Result<Vec<WorkspaceListItem>, String> {
    let resp = Request::get(&format!("{API}/workspaces"))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json::<Vec<WorkspaceListItem>>()
        .await
        .map_err(|e| e.to_string())
}

async fn api_activate_workspace(workspace_id: Uuid, token: &str) -> Result<String, String> {
    let resp = Request::post(&format!("{API}/workspaces/{workspace_id}/activate"))
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err("Failed to switch workspace".into());
    }
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    Ok(body
        .get("access_token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned())
}

/// Dropdown that lists all workspaces and lets the user switch the active one.
/// Intended to be mounted in the app shell.
#[component]
pub fn WorkspaceSwitcher() -> impl IntoView {
    let session = use_session();
    let access_token = use_access_token();
    let (open, set_open) = signal(false);

    let workspaces = LocalResource::new(move || {
        let tok = access_token.get();
        async move {
            match tok {
                Some(t) => api_list_workspaces(&t).await.unwrap_or_default(),
                None => vec![],
            }
        }
    });

    let switch = move |ws_id: Uuid| {
        let tok = access_token.get().unwrap_or_default();
        spawn_local(async move {
            if let Ok(new_token) = api_activate_workspace(ws_id, &tok).await {
                access_token.set(new_token);
                if let Some(mut user) = session.get() {
                    user.workspace_id = ws_id;
                    session.set_user(user);
                }
            }
            set_open.set(false);
        });
    };

    view! {
        <div class="relative">
            <button
                type="button"
                class="flex items-center gap-2 text-sm text-gray-700 hover:text-gray-900"
                on:click=move |_| set_open.update(|v| *v = !*v)
            >
                <span>
                    {move || session.get()
                        .map(|u| u.name)
                        .unwrap_or_else(|| "Workspace".into())}
                </span>
                <svg class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                    <path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clip-rule="evenodd"/>
                </svg>
            </button>

            {move || open.get().then(|| view! {
                <div class="absolute left-0 mt-2 w-56 bg-white border border-gray-200 rounded-xl shadow-lg z-50 py-1">
                    <Suspense fallback=move || view! {
                        <div class="px-4 py-2 text-sm text-gray-400">"Loading\u{2026}"</div>
                    }>
                        {move || workspaces.get().as_deref().map(|list| list.to_vec().into_iter().map(|ws| {
                            let ws_id = ws.id;
                            let is_active = session.get()
                                .map(|u| u.workspace_id == ws_id)
                                .unwrap_or(false);
                            view! {
                                <button
                                    type="button"
                                    class=if is_active {
                                        "w-full text-left px-4 py-2 text-sm bg-indigo-50 text-indigo-700 font-medium"
                                    } else {
                                        "w-full text-left px-4 py-2 text-sm text-gray-700 hover:bg-gray-50"
                                    }
                                    on:click=move |_| switch(ws_id)
                                >
                                    <div class="font-medium">{ws.name.clone()}</div>
                                    <div class="text-xs text-gray-400 capitalize">{ws.kind.clone()}</div>
                                </button>
                            }
                        }).collect_view())}
                    </Suspense>

                    <div class="border-t border-gray-100 mt-1 pt-1">
                        <A
                            href="/auth/onboarding"
                            attr:class="block px-4 py-2 text-sm text-indigo-600 hover:bg-indigo-50"
                        >
                            "+ New workspace"
                        </A>
                    </div>
                </div>
            })}
        </div>
    }
}
