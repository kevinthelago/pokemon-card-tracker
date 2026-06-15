//! Scalper detection settings — D2.
//!
//! Route: /settings/:wid/scalper  (seller workspaces only)

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use crate::api;

// ── DTOs (mirror API types) ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectionConfig {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub enabled: bool,
    pub velocity_window_hours: i32,
    pub velocity_threshold: i32,
    pub bulk_single_item_limit: i32,
    pub sweep_printing_limit: i32,
    pub repeat_window_minutes: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AllowlistEntry {
    pub id: Uuid,
    pub buyer_hash: String,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ── Page ──────────────────────────────────────────────────────────────────────

#[component]
pub fn ScalperSettings() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| p.get("wid").as_deref().and_then(|s| Uuid::parse_str(s).ok()))
    };

    let (reload, set_reload) = signal(0u32);

    let config = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move {
            let wid = wid?;
            api::fetch_scalper_config(wid).await.ok()
        }
    });

    let allowlist = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move {
            let wid = wid?;
            api::fetch_allowlist(wid).await.ok()
        }
    });

    let trigger_reload = move || set_reload.update(|n| *n += 1);

    view! {
        <div class="scalper-settings">
            <h1>"Scalper Detection"</h1>
            <p class="subtitle">
                "Automatically flag transactions that match bulk-buying or velocity patterns."
            </p>

            <Suspense fallback=move || view! { <p class="loading">"Loading..."</p> }>
                {move || {
                    let cfg = config.get();
                    let al = allowlist.get();
                    let wid = workspace_id();
                    match (cfg.as_deref(), al.as_deref(), wid) {
                        (Some(Some(cfg)), Some(Some(al)), Some(wid)) => {
                            let on_change = Callback::new(move |_: ()| trigger_reload());
                            view! {
                                <ConfigPanel workspace_id=wid config=cfg.clone() on_saved=on_change />
                                <AllowlistPanel workspace_id=wid entries=al.clone() on_change=on_change />
                            }.into_any()
                        }
                        (Some(None), _, _) | (_, Some(None), _) => view! {
                            <p class="error">"Failed to load scalper settings."</p>
                        }.into_any(),
                        _ => view! { <p class="loading">"Loading..."</p> }.into_any(),
                    }
                }}
            </Suspense>

            <AlertsExplainer />
        </div>
    }
}

// ── Config panel ─────────────────────────────────────────────────────────────

#[component]
fn ConfigPanel(
    workspace_id: Uuid,
    config: DetectionConfig,
    on_saved: Callback<()>,
) -> impl IntoView {
    let (saving, set_saving) = signal(false);
    let (save_error, set_save_error) = signal::<Option<String>>(None);

    view! {
        <section class="scalper-section">
            <h2>"Detection Rules"</h2>
            <ConfigForm
                workspace_id=workspace_id
                config=config
                saving=saving
                set_saving=set_saving
                set_save_error=set_save_error
                on_saved=on_saved
            />
            {move || save_error.get().map(|e| view! { <p class="error">{e}</p> })}
        </section>
    }
}

#[component]
fn ConfigForm(
    workspace_id: Uuid,
    config: DetectionConfig,
    saving: ReadSignal<bool>,
    set_saving: WriteSignal<bool>,
    set_save_error: WriteSignal<Option<String>>,
    on_saved: Callback<()>,
) -> impl IntoView {
    let (enabled, set_enabled) = signal(config.enabled);
    let (vel_window, set_vel_window) = signal(config.velocity_window_hours.to_string());
    let (vel_thresh, set_vel_thresh) = signal(config.velocity_threshold.to_string());
    let (bulk_limit, set_bulk_limit) = signal(config.bulk_single_item_limit.to_string());
    let (sweep_limit, set_sweep_limit) = signal(config.sweep_printing_limit.to_string());
    let (repeat_win, set_repeat_win) = signal(config.repeat_window_minutes.to_string());

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if saving.get() {
            return;
        }

        let body = api::UpdateConfigBody {
            enabled: Some(enabled.get()),
            velocity_window_hours: vel_window.get().parse().ok(),
            velocity_threshold: vel_thresh.get().parse().ok(),
            bulk_single_item_limit: bulk_limit.get().parse().ok(),
            sweep_printing_limit: sweep_limit.get().parse().ok(),
            repeat_window_minutes: repeat_win.get().parse().ok(),
        };

        set_saving.set(true);
        set_save_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match api::update_scalper_config(workspace_id, &body).await {
                Ok(_) => {
                    set_saving.set(false);
                    on_saved.run(());
                }
                Err(e) => {
                    set_saving.set(false);
                    set_save_error.set(Some(e));
                }
            }
        });
    };

    view! {
        <form class="config-form" on:submit=on_submit>
            <label class="field">
                <span class="field__label">"Enable scalper detection"</span>
                <input
                    type="checkbox"
                    checked=move || enabled.get()
                    on:change=move |ev| set_enabled.set(event_target_checked(&ev))
                />
            </label>
            <label class="field">
                <span class="field__label">"Velocity window (hours)"</span>
                <input type="number" min="1" max="720"
                    prop:value=move || vel_window.get()
                    on:input=move |ev| set_vel_window.set(event_target_value(&ev)) />
            </label>
            <label class="field">
                <span class="field__label">"Velocity threshold (transactions)"</span>
                <input type="number" min="1"
                    prop:value=move || vel_thresh.get()
                    on:input=move |ev| set_vel_thresh.set(event_target_value(&ev)) />
            </label>
            <label class="field">
                <span class="field__label">"Bulk single-item limit"</span>
                <input type="number" min="1"
                    prop:value=move || bulk_limit.get()
                    on:input=move |ev| set_bulk_limit.set(event_target_value(&ev)) />
            </label>
            <label class="field">
                <span class="field__label">"Sweep printing limit"</span>
                <input type="number" min="1"
                    prop:value=move || sweep_limit.get()
                    on:input=move |ev| set_sweep_limit.set(event_target_value(&ev)) />
            </label>
            <label class="field">
                <span class="field__label">"Repeat window (minutes)"</span>
                <input type="number" min="1" max="1440"
                    prop:value=move || repeat_win.get()
                    on:input=move |ev| set_repeat_win.set(event_target_value(&ev)) />
            </label>
            <button type="submit" disabled=move || saving.get()>
                {move || if saving.get() { "Saving..." } else { "Save" }}
            </button>
        </form>
    }
}

// ── Allowlist panel ───────────────────────────────────────────────────────────

#[component]
fn AllowlistPanel(
    workspace_id: Uuid,
    entries: Vec<AllowlistEntry>,
    on_change: Callback<()>,
) -> impl IntoView {
    let (error, set_error) = signal::<Option<String>>(None);

    view! {
        <section class="scalper-section">
            <h2>"Trusted Buyer Allowlist"</h2>
            <p class="hint">
                "Buyers on this list are never flagged. Add their SHA-256 buyer hash below."
            </p>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
            {if entries.is_empty() {
                view! { <p class="empty-state">"No buyers on the allowlist yet."</p> }.into_any()
            } else {
                view! {
                    <AllowlistTable
                        workspace_id=workspace_id
                        entries=entries.clone()
                        set_error=set_error
                        on_change=on_change
                    />
                }.into_any()
            }}
            <AddAllowlistForm workspace_id=workspace_id set_error=set_error on_added=on_change />
        </section>
    }
}

#[component]
fn AllowlistTable(
    workspace_id: Uuid,
    entries: Vec<AllowlistEntry>,
    set_error: WriteSignal<Option<String>>,
    on_change: Callback<()>,
) -> impl IntoView {
    let rows = entries
        .into_iter()
        .map(|entry| {
            let entry_id = entry.id;
            let display_hash = truncate_hash(&entry.buyer_hash);
            let full_hash = entry.buyer_hash.clone();
            let notes = entry.notes.clone().unwrap_or_default();

            let on_remove = move |_| {
                wasm_bindgen_futures::spawn_local(async move {
                    match api::remove_allowlist_entry(workspace_id, entry_id).await {
                        Ok(_) => on_change.run(()),
                        Err(e) => set_error.set(Some(e)),
                    }
                });
            };

            view! {
                <tr>
                    <td class="hash" title=full_hash>{display_hash}</td>
                    <td>{notes}</td>
                    <td><button class="btn-danger" on:click=on_remove>"Remove"</button></td>
                </tr>
            }
        })
        .collect::<Vec<_>>();

    view! {
        <table class="allowlist-table">
            <thead>
                <tr>
                    <th>"Buyer hash"</th>
                    <th>"Notes"</th>
                    <th></th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
    }
}

#[component]
fn AddAllowlistForm(
    workspace_id: Uuid,
    set_error: WriteSignal<Option<String>>,
    on_added: Callback<()>,
) -> impl IntoView {
    let (hash, set_hash) = signal(String::new());
    let (notes, set_notes) = signal(String::new());
    let (adding, set_adding) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let h = hash.get();
        if h.trim().is_empty() || adding.get() {
            return;
        }

        let body = api::AddAllowlistBody {
            buyer_hash: h.trim().to_string(),
            notes: {
                let n = notes.get();
                if n.trim().is_empty() { None } else { Some(n.trim().to_string()) }
            },
        };

        set_adding.set(true);
        set_error.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match api::add_allowlist_entry(workspace_id, &body).await {
                Ok(_) => {
                    set_adding.set(false);
                    set_hash.set(String::new());
                    set_notes.set(String::new());
                    on_added.run(());
                }
                Err(e) => {
                    set_adding.set(false);
                    set_error.set(Some(e));
                }
            }
        });
    };

    view! {
        <form class="add-allowlist-form" on:submit=on_submit>
            <input type="text" placeholder="SHA-256 buyer hash"
                prop:value=move || hash.get()
                on:input=move |ev| set_hash.set(event_target_value(&ev))
                required />
            <input type="text" placeholder="Notes (optional)"
                prop:value=move || notes.get()
                on:input=move |ev| set_notes.set(event_target_value(&ev)) />
            <button type="submit" disabled=move || adding.get()>
                {move || if adding.get() { "Adding..." } else { "Add to allowlist" }}
            </button>
        </form>
    }
}

#[component]
fn AlertsExplainer() -> impl IntoView {
    view! {
        <section class="scalper-section scalper-section--info">
            <h2>"How alerts work"</h2>
            <ul>
                <li>"Flags appear in the Risk Dashboard under the Scalper category."</li>
                <li>"Workspace owners and staff receive email notifications for new flags."</li>
                <li>"Dismissing a flag records your review but does not block the buyer."</li>
                <li>"Add a buyer to the allowlist to permanently suppress their flags."</li>
            </ul>
        </section>
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn truncate_hash(h: &str) -> String {
    if h.len() <= 16 {
        h.to_string()
    } else {
        format!("{}...{}", &h[..8], &h[h.len() - 4..])
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_hash;

    #[test]
    fn truncate_short_hash_unchanged() {
        assert_eq!(truncate_hash("abc"), "abc");
        assert_eq!(truncate_hash("1234567890123456"), "1234567890123456");
    }

    #[test]
    fn truncate_long_hash() {
        let h = "abcdef1234567890abcdef1234567890";
        let t = truncate_hash(h);
        assert!(t.contains("..."));
        assert!(t.starts_with("abcdef12"));
        assert!(t.ends_with("7890"));
    }
}
