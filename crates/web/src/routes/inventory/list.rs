//! M2 — Inventory list page (CSR).
//!
//! Virtualized infinite-scroll list unifying raw (qty-tracked) and graded
//! (unique) items. Filter bar narrows the result set; bulk-select enables
//! bulk edit and delete.

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use crate::{
    api,
    routes::inventory::{BulkOp, InventoryFilter, InventoryItem},
};

// ─── Page ─────────────────────────────────────────────────────────────────────

#[component]
pub fn InventoryListPage() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| p.get("wid").as_deref().and_then(|s| Uuid::parse_str(s).ok()))
    };

    let (filter, set_filter) = signal(InventoryFilter::new());
    let (items, set_items) = signal(Vec::<InventoryItem>::new());
    let (next_cursor, set_next_cursor) = signal(Option::<String>::None);
    let (is_loading, set_loading) = signal(false);
    let (selected, set_selected) = signal(HashSet::<Uuid>::new());
    let (bulk_msg, set_bulk_msg) = signal(Option::<String>::None);

    // Load (or reload) from scratch whenever the filter changes.
    Effect::new(move || {
        let f = filter.get();
        let wid = workspace_id();
        set_items.set(Vec::new());
        set_next_cursor.set(None);
        set_selected.set(HashSet::new());

        if let Some(wid) = wid {
            set_loading.set(true);
            let f = f.clone();
            leptos::task::spawn_local(async move {
                match api::fetch_inventory(wid, &f, None).await {
                    Ok(page) => {
                        set_items.update(|v| v.extend(page.items));
                        set_next_cursor.set(page.next_cursor);
                    }
                    Err(e) => leptos::logging::error!("inventory load: {e}"),
                }
                set_loading.set(false);
            });
        }
    });

    // Load next page.
    let load_more = move || {
        let wid = workspace_id();
        let cursor = next_cursor.get();
        let f = filter.get();
        if is_loading.get() || cursor.is_none() {
            return;
        }
        if let (Some(wid), Some(cursor)) = (wid, cursor) {
            set_loading.set(true);
            let f = f.clone();
            leptos::task::spawn_local(async move {
                match api::fetch_inventory(wid, &f, Some(&cursor)).await {
                    Ok(page) => {
                        set_items.update(|v| v.extend(page.items));
                        set_next_cursor.set(page.next_cursor);
                    }
                    Err(e) => leptos::logging::error!("inventory load more: {e}"),
                }
                set_loading.set(false);
            });
        }
    };

    // Bulk delete.
    let bulk_delete = move || {
        let ids: Vec<Uuid> = selected.get().into_iter().collect();
        let wid = workspace_id();
        if ids.is_empty() || wid.is_none() {
            return;
        }
        let wid = wid.unwrap();
        let op = BulkOp::Delete { ids: ids.clone() };
        set_bulk_msg.set(None);
        leptos::task::spawn_local(async move {
            match api::bulk_inventory(wid, &op).await {
                Ok(n) => {
                    set_items.update(|v| v.retain(|i| !ids.contains(&i.id)));
                    set_selected.set(HashSet::new());
                    set_bulk_msg.set(Some(format!("Deleted {n} item(s).")));
                }
                Err(e) => set_bulk_msg.set(Some(format!("Delete failed: {e}"))),
            }
        });
    };

    let toggle_select = move |id: Uuid| {
        set_selected.update(|s| {
            if s.contains(&id) { s.remove(&id); } else { s.insert(id); }
        });
    };

    let select_all = move || {
        let ids: HashSet<Uuid> = items.get().iter().map(|i| i.id).collect();
        set_selected.set(ids);
    };

    let clear_selection = move || set_selected.set(HashSet::new());

    view! {
        <div class="inventory-page">
            <header class="inventory-page__header">
                <h1>"Inventory"</h1>
            </header>

            <FilterBar filter=filter set_filter=set_filter />

            {move || {
                let sel_count = selected.get().len();
                if sel_count > 0 {
                    view! {
                        <BulkActionBar
                            count=sel_count
                            on_delete=Callback::new(move |_: ()| bulk_delete())
                            on_clear=Callback::new(move |_: ()| clear_selection())
                            msg=bulk_msg
                        />
                    }.into_any()
                } else {
                    view! { <div /> }.into_any()
                }
            }}

            <div class="inventory-toolbar">
                <button
                    class="btn btn--sm"
                    on:click=move |_| select_all()
                >"Select all"</button>
            </div>

            <InventoryTable
                items=items
                selected=selected
                on_toggle=Callback::new(move |id: Uuid| toggle_select(id))
                workspace_id=Signal::derive(workspace_id)
            />

            {move || {
                if is_loading.get() {
                    view! { <p class="inventory-page__loading">"Loading…"</p> }.into_any()
                } else if next_cursor.get().is_some() {
                    view! {
                        <IntersectionSentinel on_intersect=Callback::new(move |_: ()| load_more()) />
                    }.into_any()
                } else if items.get().is_empty() {
                    view! { <EmptyState /> }.into_any()
                } else {
                    view! { <div /> }.into_any()
                }
            }}
        </div>
    }
}

// ─── Filter bar ───────────────────────────────────────────────────────────────

#[component]
fn FilterBar(
    filter: ReadSignal<InventoryFilter>,
    set_filter: WriteSignal<InventoryFilter>,
) -> impl IntoView {
    view! {
        <div class="filter-bar">
            <input
                type="search"
                class="filter-bar__search"
                placeholder="Search cards…"
                prop:value=move || filter.get().search.clone().unwrap_or_default()
                on:input=move |ev| {
                    let val = event_target_value(&ev);
                    set_filter.update(|f| {
                        f.search = if val.is_empty() { None } else { Some(val) };
                    });
                }
            />
            <select
                class="filter-bar__kind"
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    set_filter.update(|f| {
                        f.kind = if val.is_empty() { None } else { Some(val) };
                    });
                }
            >
                <option value="">"All types"</option>
                <option value="raw">"Raw"</option>
                <option value="graded">"Graded"</option>
            </select>
            <label class="filter-bar__risk">
                <input
                    type="checkbox"
                    prop:checked=move || filter.get().risk_flagged
                    on:change=move |ev| {
                        let checked = event_target_checked(&ev);
                        set_filter.update(|f| f.risk_flagged = checked);
                    }
                />
                " Risk flagged"
            </label>
            <select
                class="filter-bar__sort"
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    set_filter.update(|f| f.sort = val);
                }
            >
                <option value="date">"Date added"</option>
                <option value="value">"Value"</option>
                <option value="name">"Name"</option>
            </select>
            <select
                class="filter-bar__order"
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    set_filter.update(|f| f.order = val);
                }
            >
                <option value="desc">"Newest first"</option>
                <option value="asc">"Oldest first"</option>
            </select>
        </div>
    }
}

// ─── Bulk action bar ──────────────────────────────────────────────────────────

#[component]
fn BulkActionBar(
    count: usize,
    on_delete: Callback<()>,
    on_clear: Callback<()>,
    msg: ReadSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="bulk-bar">
            <span class="bulk-bar__count">{count}" selected"</span>
            <button class="btn btn--danger btn--sm" on:click=move |_| on_delete.run(())>
                "Delete"
            </button>
            <button class="btn btn--sm" on:click=move |_| on_clear.run(())>
                "Clear"
            </button>
            {move || msg.get().map(|m| view! { <span class="bulk-bar__msg">{m}</span> })}
        </div>
    }
}

// ─── Inventory table ──────────────────────────────────────────────────────────

#[component]
fn InventoryTable(
    items: ReadSignal<Vec<InventoryItem>>,
    selected: ReadSignal<HashSet<Uuid>>,
    on_toggle: Callback<Uuid>,
    workspace_id: Signal<Option<Uuid>>,
) -> impl IntoView {
    view! {
        <div class="inventory-table">
            <div class="inventory-table__header">
                <span class="col-check" />
                <span class="col-image" />
                <span class="col-name">"Name"</span>
                <span class="col-type">"Type"</span>
                <span class="col-qty">"Qty"</span>
                <span class="col-value">"Value"</span>
                <span class="col-risk">"Risk"</span>
            </div>
            <For
                each=move || items.get()
                key=|item| item.id
                children=move |item| {
                    let id = item.id;
                    let is_selected = move || selected.get().contains(&id);
                    let wid = workspace_id;
                    view! {
                        <InventoryRowView
                            item=item
                            is_selected=Signal::derive(is_selected)
                            on_toggle=Callback::new(move |_: ()| on_toggle.run(id))
                            workspace_id=wid
                        />
                    }
                }
            />
        </div>
    }
}

// ─── Single row ───────────────────────────────────────────────────────────────

#[component]
fn InventoryRowView(
    item: InventoryItem,
    is_selected: Signal<bool>,
    on_toggle: Callback<()>,
    workspace_id: Signal<Option<Uuid>>,
) -> impl IntoView {
    let id = item.id;
    let href = move || {
        workspace_id
            .get()
            .map(|wid| format!("/workspaces/{wid}/inventory/{id}"))
            .unwrap_or_default()
    };

    let value_str = item
        .current_value
        .map(|v| format!("${v}"))
        .unwrap_or_default();
    let risk_badge = if item.has_risk_flag {
        Some(view! { <span class="badge badge--risk">"Risk"</span> })
    } else {
        None
    };
    let kind_badge = if item.kind == "graded" {
        view! { <span class="badge badge--graded">"PSA/CGC"</span> }
    } else {
        view! { <span class="badge badge--raw">"Raw"</span> }
    };
    let qty = if item.kind == "graded" {
        item.grade.clone().unwrap_or_else(|| "—".into())
    } else {
        item.quantity.to_string()
    };

    view! {
        <div class="inventory-row" class:inventory-row--selected=is_selected>
            <span class="col-check">
                <input
                    type="checkbox"
                    prop:checked=is_selected
                    on:change=move |_| on_toggle.run(())
                />
            </span>
            <span class="col-image">
                {item.image_url.as_ref().map(|url| {
                    let url = url.clone();
                    view! { <img src=url alt="" class="card-thumb" /> }
                })}
            </span>
            <span class="col-name">
                <a href=href>{item.printing_name.clone()}</a>
                <span class="card-meta">{item.set_code.clone()}" · "{item.collector_number.clone()}</span>
            </span>
            <span class="col-type">{kind_badge}</span>
            <span class="col-qty">{qty}</span>
            <span class="col-value">{value_str}</span>
            <span class="col-risk">{risk_badge}</span>
        </div>
    }
}

// ─── Intersection sentinel (infinite scroll) ──────────────────────────────────

#[component]
fn IntersectionSentinel(on_intersect: Callback<()>) -> impl IntoView {
    use leptos::html::Div;
    let node_ref = NodeRef::<Div>::new();

    Effect::new(move || {
        if let Some(el) = node_ref.get() {
            let cb = on_intersect;
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::closure::Closure;
                use wasm_bindgen::JsCast;

                let f = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
                    for entry in entries.iter() {
                        let entry: web_sys::IntersectionObserverEntry = entry.unchecked_into();
                        if entry.is_intersecting() {
                            cb.run(());
                        }
                    }
                });
                let opts = web_sys::IntersectionObserverInit::new();
                if let Ok(observer) = web_sys::IntersectionObserver::new_with_options(
                    f.as_ref().unchecked_ref(),
                    &opts,
                ) {
                    observer.observe(&el);
                }
                f.forget();
            }
            let _ = el; // suppress unused warning in non-wasm builds
        }
    });

    view! { <div node_ref=node_ref class="intersection-sentinel" /> }
}

// ─── Empty state ──────────────────────────────────────────────────────────────

#[component]
fn EmptyState() -> impl IntoView {
    view! {
        <div class="empty-state">
            <p class="empty-state__msg">"No inventory items yet."</p>
        </div>
    }
}
