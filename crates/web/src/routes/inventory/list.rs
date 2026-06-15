//! M2 — Inventory list page (CSR).
//!
//! Virtualized infinite-scroll list unifying raw (qty-tracked) and graded
//! (unique) items.  Filter bar narrows the result set; bulk-select enables
//! bulk edit and delete.  Density toggle switches between comfortable and
//! compact row layouts.  Export button triggers a CSV download.

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{
    api,
    routes::inventory::{BulkOp, InventoryFilter, InventoryItem},
};

// ─── Page ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
enum Density {
    Comfortable,
    Compact,
}

impl Density {
    fn class(self) -> &'static str {
        match self {
            Density::Comfortable => "inventory-table--comfortable",
            Density::Compact => "inventory-table--compact",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Density::Comfortable => "Compact",
            Density::Compact => "Comfortable",
        }
    }
    fn toggle(self) -> Density {
        match self {
            Density::Comfortable => Density::Compact,
            Density::Compact => Density::Comfortable,
        }
    }
}

#[component]
pub fn InventoryListPage() -> impl IntoView {
    let params = use_params_map();
    let workspace_id = move || {
        params.with(|p| {
            p.get("wid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
        })
    };

    let (filter, set_filter) = signal(InventoryFilter::new());
    let (items, set_items) = signal(Vec::<InventoryItem>::new());
    let (next_cursor, set_next_cursor) = signal(Option::<String>::None);
    let (is_loading, set_loading) = signal(false);
    let (selected, set_selected) = signal(HashSet::<Uuid>::new());
    let (bulk_msg, set_bulk_msg) = signal(Option::<String>::None);
    let (density, set_density) = signal(Density::Comfortable);
    let (show_bulk_edit, set_show_bulk_edit) = signal(false);

    // Load (or reload) from scratch whenever the filter changes.
    Effect::new(move || {
        let f = filter.get();
        let wid = workspace_id();
        set_items.set(Vec::new());
        set_next_cursor.set(None);
        set_selected.set(HashSet::new());
        set_show_bulk_edit.set(false);

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
            if s.contains(&id) {
                s.remove(&id);
            } else {
                s.insert(id);
            }
        });
    };

    let select_all = move || {
        let ids: HashSet<Uuid> = items.get().iter().map(|i| i.id).collect();
        set_selected.set(ids);
    };

    let clear_selection = move || {
        set_selected.set(HashSet::new());
        set_show_bulk_edit.set(false);
    };

    // Export URL derived from current filter + workspace.
    let export_url = move || {
        let wid = workspace_id()?;
        let f = filter.get();
        let mut url = format!("/api/workspaces/{wid}/catalogue/items/export?limit=10000");
        if let Some(s) = &f.search {
            url.push_str(&format!("&search={s}"));
        }
        if let Some(k) = &f.kind {
            url.push_str(&format!("&kind={k}"));
        }
        if let Some(sc) = &f.set_code {
            url.push_str(&format!("&set_code={sc}"));
        }
        if let Some(r) = &f.rarity {
            url.push_str(&format!("&rarity={r}"));
        }
        if let Some(c) = &f.condition {
            url.push_str(&format!("&condition={c}"));
        }
        if f.risk_flagged {
            url.push_str("&risk_flagged=true");
        }
        url.push_str(&format!("&sort={}&order={}", f.sort, f.order));
        Some(url)
    };

    view! {
        <div class="inventory-page">
            <header class="inventory-page__header">
                <h1>"Inventory"</h1>
                <div class="inventory-page__header-actions">
                    {move || export_url().map(|url| view! {
                        <a
                            href=url
                            download="inventory.csv"
                            class="btn btn--sm"
                        >
                            "Export CSV"
                        </a>
                    })}
                    <button
                        class="btn btn--sm"
                        on:click=move |_| set_density.update(|d| *d = d.toggle())
                    >
                        {move || density.get().label()}
                    </button>
                </div>
            </header>

            <FilterBar filter=filter set_filter=set_filter />

            {move || {
                let sel_count = selected.get().len();
                if sel_count > 0 {
                    view! {
                        <BulkActionBar
                            count=sel_count
                            show_edit=show_bulk_edit
                            on_toggle_edit=Callback::new(move |_: ()| {
                                set_show_bulk_edit.update(|v| *v = !*v);
                            })
                            on_delete=Callback::new(move |_: ()| bulk_delete())
                            on_clear=Callback::new(move |_: ()| clear_selection())
                            msg=bulk_msg
                        />
                    }.into_any()
                } else {
                    view! { <div /> }.into_any()
                }
            }}

            {move || {
                if show_bulk_edit.get() {
                    let wid = workspace_id();
                    let ids: Vec<Uuid> = selected.get().into_iter().collect();
                    view! {
                        <BulkEditForm
                            ids=ids
                            workspace_id=wid
                            on_done=Callback::new(move |msg: String| {
                                set_bulk_msg.set(Some(msg));
                                set_show_bulk_edit.set(false);
                                // Reload the list to reflect edits.
                                set_filter.update(|f| { let _ = f; });
                            })
                            on_cancel=Callback::new(move |_: ()| set_show_bulk_edit.set(false))
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
                density=density
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
                        use wasm_bindgen::JsCast;
                        let checked = ev.target()
                            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                            .map(|i| i.checked())
                            .unwrap_or(false);
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
    show_edit: ReadSignal<bool>,
    on_toggle_edit: Callback<()>,
    on_delete: Callback<()>,
    on_clear: Callback<()>,
    msg: ReadSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="bulk-bar">
            <span class="bulk-bar__count">{count}" selected"</span>
            <button
                class="btn btn--sm"
                class:btn--active=move || show_edit.get()
                on:click=move |_| on_toggle_edit.run(())
            >
                "Edit"
            </button>
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

// ─── Bulk edit form ───────────────────────────────────────────────────────────

#[component]
fn BulkEditForm(
    ids: Vec<Uuid>,
    workspace_id: Option<Uuid>,
    on_done: Callback<String>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let (condition, set_condition) = signal(String::new());
    let (acq_cost, set_acq_cost) = signal(String::new());
    let (err, set_err) = signal(Option::<String>::None);

    let submit = move || {
        let wid = match workspace_id {
            Some(w) => w,
            None => return,
        };
        let cond = condition.get();
        let op = BulkOp::Edit {
            ids: ids.clone(),
            condition: if cond.is_empty() { None } else { Some(cond) },
            acquisition_cost: acq_cost.get().parse::<Decimal>().ok(),
        };
        leptos::task::spawn_local(async move {
            match api::bulk_inventory(wid, &op).await {
                Ok(n) => on_done.run(format!("Updated {n} item(s).")),
                Err(e) => set_err.set(Some(format!("Edit failed: {e}"))),
            }
        });
    };

    view! {
        <form
            class="bulk-edit-form"
            on:submit=move |ev| { ev.prevent_default(); submit(); }
        >
            <span class="bulk-edit-form__title">"Bulk edit"</span>

            <label class="field">
                "Condition (leave blank to keep)"
                <select on:change=move |ev| set_condition.set(event_target_value(&ev))>
                    <option value="">"— no change —"</option>
                    <option value="mint">"Mint"</option>
                    <option value="near_mint">"Near Mint"</option>
                    <option value="lightly_played">"Lightly Played"</option>
                    <option value="moderately_played">"Moderately Played"</option>
                    <option value="heavily_played">"Heavily Played"</option>
                    <option value="damaged">"Damaged"</option>
                </select>
            </label>

            <label class="field">
                "Acquisition cost (leave blank to keep)"
                <input
                    type="number"
                    step="0.01"
                    placeholder="0.00"
                    prop:value=move || acq_cost.get()
                    on:input=move |ev| set_acq_cost.set(event_target_value(&ev))
                />
            </label>

            {move || err.get().map(|e| view! { <p class="edit-form__error">{e}</p> })}

            <div class="bulk-edit-form__actions">
                <button type="submit" class="btn btn--primary btn--sm">"Apply"</button>
                <button
                    type="button"
                    class="btn btn--sm"
                    on:click=move |_| on_cancel.run(())
                >
                    "Cancel"
                </button>
            </div>
        </form>
    }
}

// ─── Inventory table ──────────────────────────────────────────────────────────

#[component]
fn InventoryTable(
    items: ReadSignal<Vec<InventoryItem>>,
    selected: ReadSignal<HashSet<Uuid>>,
    density: ReadSignal<Density>,
    on_toggle: Callback<Uuid>,
    workspace_id: Signal<Option<Uuid>>,
) -> impl IntoView {
    view! {
        <div class="inventory-table" class:inventory-table--compact=move || density.get() == Density::Compact>
            <div class="inventory-table__header">
                <span class="col-check" />
                <span class="col-image" />
                <span class="col-name">"Name"</span>
                <span class="col-type">"Type"</span>
                <span class="col-qty">"Qty / Grade"</span>
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
            <a href="/catalogue/add" class="btn btn--primary">"Add a card"</a>
        </div>
    }
}
