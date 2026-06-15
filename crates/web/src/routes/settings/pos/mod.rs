use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api;

// ---------------------------------------------------------------------------
// DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PosConnectionDto {
    pub id: Uuid,
    pub provider: String,
    pub provider_display: String,
    pub external_account_id: Option<String>,
    pub status: String,
    pub uses_polling: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

// ---------------------------------------------------------------------------
// Root page
// ---------------------------------------------------------------------------

#[component]
pub fn PosSettingsPage() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| {
            p.get("wid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_default()
        })
    };

    let (reload, set_reload) = signal(0u32);

    let connections = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move { api::fetch_pos_connections(wid).await.ok().unwrap_or_default() }
    });

    let (disconnect_error, set_disconnect_error) = signal(Option::<String>::None);

    let do_disconnect = move |connection_id: Uuid| {
        let wid = workspace_id();
        set_disconnect_error.set(None);
        leptos::task::spawn_local(async move {
            match api::disconnect_pos(wid, connection_id).await {
                Ok(()) => set_reload.update(|n| *n += 1),
                Err(e) => set_disconnect_error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="space-y-6 p-6">
            <div>
                <h1 class="text-2xl font-semibold text-gray-900">"POS Connections"</h1>
                <p class="mt-1 text-sm text-gray-500">
                    "Connect your point-of-sale system to sync inventory and sales automatically."
                </p>
            </div>

            <ConnectSection workspace_id=workspace_id />

            {move || disconnect_error.get().map(|e| view! {
                <div class="bg-red-50 border border-red-200 rounded p-3 text-red-700 text-sm">
                    {format!("Disconnect failed: {e}")}
                </div>
            })}

            <Suspense fallback=|| view! {
                <div class="text-gray-400 text-sm p-4">"Loading connections…"</div>
            }>
                {move || {
                    connections.get().map(|conns| {
                        if conns.is_empty() {
                            view! { <EmptyState /> }.into_any()
                        } else {
                            view! {
                                <ConnectionList connections=conns.to_vec() on_disconnect=do_disconnect />
                            }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Connect section
// ---------------------------------------------------------------------------

#[component]
fn ConnectSection(workspace_id: impl Fn() -> Uuid + 'static + Copy) -> impl IntoView {
    view! {
        <div class="bg-white border border-gray-200 rounded-lg p-6">
            <h2 class="text-base font-medium text-gray-900 mb-4">"Add a connection"</h2>
            <div class="flex flex-wrap gap-3">
                <ProviderButton
                    provider="square"
                    label="Square"
                    icon="▪"
                    color="bg-black"
                    workspace_id=workspace_id
                />
                <ProviderButton
                    provider="shopify"
                    label="Shopify POS"
                    icon="🛍"
                    color="bg-green-600"
                    workspace_id=workspace_id
                />
                <ProviderButton
                    provider="clover"
                    label="Clover"
                    icon="🍀"
                    color="bg-green-700"
                    workspace_id=workspace_id
                />
            </div>
        </div>
    }
}

#[component]
fn ProviderButton(
    provider: &'static str,
    label: &'static str,
    icon: &'static str,
    color: &'static str,
    workspace_id: impl Fn() -> Uuid + 'static + Copy,
) -> impl IntoView {
    let (pending, set_pending) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let onclick = move |_| {
        let wid = workspace_id();
        set_pending.set(true);
        set_error.set(None);
        leptos::task::spawn_local(async move {
            match api::start_pos_oauth(wid, provider).await {
                Ok(url) => {
                    // Navigate to the OAuth URL
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href(&url);
                    }
                }
                Err(e) => {
                    set_pending.set(false);
                    set_error.set(Some(e));
                }
            }
        });
    };

    view! {
        <div>
            <button
                class=format!(
                    "flex items-center gap-2 px-4 py-2 rounded-md text-white text-sm font-medium \
                     {color} hover:opacity-90 disabled:opacity-50 transition-opacity"
                )
                on:click=onclick
                disabled=move || pending.get()
            >
                <span>{icon}</span>
                {move || if pending.get() {
                    "Connecting…".to_string()
                } else {
                    format!("Connect {label}")
                }}
            </button>
            {move || error.get().map(|e| view! {
                <p class="mt-1 text-xs text-red-600">{e}</p>
            })}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Connection list
// ---------------------------------------------------------------------------

#[component]
fn ConnectionList(
    connections: Vec<PosConnectionDto>,
    on_disconnect: impl Fn(Uuid) + 'static + Clone,
) -> impl IntoView {
    view! {
        <div class="space-y-3">
            {connections
                .into_iter()
                .map(|conn| {
                    let on_disc = on_disconnect.clone();
                    view! { <ConnectionCard conn=conn on_disconnect=on_disc /> }
                })
                .collect_view()}
        </div>
    }
}

#[component]
fn ConnectionCard(conn: PosConnectionDto, on_disconnect: impl Fn(Uuid) + 'static) -> impl IntoView {
    let id = conn.id;
    let status_class = match conn.status.as_str() {
        "active" => "bg-green-100 text-green-700",
        "error" => "bg-red-100 text-red-700",
        "pending" => "bg-yellow-100 text-yellow-700",
        _ => "bg-gray-100 text-gray-600",
    };
    let status_label = match conn.status.as_str() {
        "active" => "Active",
        "error" => "Error",
        "pending" => "Connecting…",
        _ => "Disconnected",
    };
    let sync_label = if conn.uses_polling { "Polling" } else { "Webhook" };
    let last_sync = conn
        .last_synced_at
        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "Never".to_string());

    view! {
        <div class="bg-white border border-gray-200 rounded-lg p-4 flex items-start justify-between gap-4">
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                    <span class="font-medium text-gray-900">{conn.provider_display}</span>
                    <span class=format!("text-xs px-2 py-0.5 rounded-full font-medium {status_class}")>
                        {status_label}
                    </span>
                    <span class="text-xs text-gray-400">{sync_label}</span>
                </div>

                {conn.external_account_id.map(|m| view! {
                    <p class="text-xs text-gray-500 mt-0.5">"Account: " {m}</p>
                })}

                <p class="text-xs text-gray-400 mt-1">"Last synced: " {last_sync}</p>

                {conn.error_message.map(|msg| view! {
                    <p class="mt-2 text-xs text-red-600">{msg}</p>
                })}
            </div>

            <button
                class="text-sm text-red-600 hover:text-red-800 whitespace-nowrap"
                on:click=move |_| on_disconnect(id)
            >
                "Disconnect"
            </button>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

#[component]
fn EmptyState() -> impl IntoView {
    view! {
        <div class="bg-gray-50 border border-dashed border-gray-300 rounded-lg p-8 text-center">
            <div class="text-4xl mb-3">"🔌"</div>
            <h3 class="text-base font-medium text-gray-900">"No POS connected"</h3>
            <p class="mt-1 text-sm text-gray-500">
                "Connect a POS system above to sync your inventory automatically."
            </p>
        </div>
    }
}
