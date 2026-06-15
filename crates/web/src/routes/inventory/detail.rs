//! M2 — Inventory detail/edit page (CSR).
//!
//! Shows a single inventory item. Raw items are fully editable (condition,
//! quantity, acquisition cost, notes, current value). Graded items lock the
//! identity fields (grade, grader, cert number) and do not expose quantity.
//! Conflicts (409) are surfaced as a reload prompt.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{
    api,
    routes::inventory::{InventoryItem, PatchRequest},
};

// ─── Page ─────────────────────────────────────────────────────────────────────

#[component]
pub fn InventoryDetailPage() -> impl IntoView {
    let params = use_params_map();

    let workspace_id = move || {
        params.with(|p| p.get("wid").as_deref().and_then(|s| Uuid::parse_str(s).ok()))
    };
    let item_id = move || {
        params.with(|p| p.get("id").as_deref().and_then(|s| Uuid::parse_str(s).ok()))
    };

    let (reload, set_reload) = signal(0u32);

    let item = LocalResource::new(move || {
        let wid = workspace_id();
        let id = item_id();
        let _ = reload.get();
        async move {
            let (wid, id) = (wid?, id?);
            api::fetch_inventory_item(wid, id).await.ok()
        }
    });

    let refetch = move || set_reload.update(|n| *n += 1);

    view! {
        <div class="detail-page">
            <a
                href=move || {
                    workspace_id()
                        .map(|wid| format!("/workspaces/{wid}/inventory"))
                        .unwrap_or_default()
                }
                class="detail-page__back"
            >
                "← Back"
            </a>

            <Suspense fallback=move || view! { <p class="loading">"Loading…"</p> }>
                {move || {
                    match item.get().as_deref() {
                        None => view! { <p class="loading">"Loading…"</p> }.into_any(),
                        Some(None) => {
                            view! { <p class="error">"Item not found."</p> }.into_any()
                        }
                        Some(Some(data)) => {
                            view! {
                                <DetailView
                                    item=data.clone()
                                    workspace_id=workspace_id().unwrap_or_default()
                                    on_reload=Callback::new(move |_: ()| refetch())
                                />
                            }
                            .into_any()
                        }
                    }
                }}
            </Suspense>
        </div>
    }
}

// ─── Detail view (read-only / edit toggle) ────────────────────────────────────

#[derive(Clone, PartialEq)]
enum StatusMsg {
    Success(String),
    Error(String),
    Conflict,
}

#[component]
fn DetailView(
    item: InventoryItem,
    workspace_id: Uuid,
    on_reload: Callback<()>,
) -> impl IntoView {
    let (editing, set_editing) = signal(false);
    let (status, set_status) = signal(Option::<StatusMsg>::None);

    let item_sig = StoredValue::new(item.clone());

    let handle_delete = {
        let id = item.id;
        move || {
            let wid = workspace_id;
            leptos::task::spawn_local(async move {
                match api::delete_inventory_item(wid, id).await {
                    Ok(()) => on_reload.run(()),
                    Err(e) => set_status.set(Some(StatusMsg::Error(format!("Delete failed: {e}")))),
                }
            });
        }
    };

    view! {
        <div class="detail-view">
            <h1 class="detail-view__title">{item.printing_name.clone()}</h1>
            <span class="detail-view__subtitle">
                {item.set_name.clone()}" · "{item.set_code.clone()}" #"{item.collector_number.clone()}
            </span>

            {move || status.get().map(|s| match s {
                StatusMsg::Success(m) => view! {
                    <div class="status-msg status-msg--success">{m}</div>
                }.into_any(),
                StatusMsg::Error(m) => view! {
                    <div class="status-msg status-msg--error">{m}</div>
                }.into_any(),
                StatusMsg::Conflict => view! {
                    <div class="status-msg status-msg--conflict">
                        "This item was modified elsewhere. "
                        <button class="btn btn--sm" on:click=move |_| on_reload.run(())>
                            "Reload"
                        </button>
                    </div>
                }.into_any(),
            })}

            {move || {
                if editing.get() {
                    let item_val = item_sig.get_value();
                    view! {
                        <EditForm
                            item=item_val
                            workspace_id=workspace_id
                            on_saved=Callback::new(move |_: ()| {
                                set_editing.set(false);
                                set_status.set(Some(StatusMsg::Success("Saved.".into())));
                                on_reload.run(());
                            })
                            on_conflict=Callback::new(move |_: ()| {
                                set_editing.set(false);
                                set_status.set(Some(StatusMsg::Conflict));
                            })
                            on_cancel=Callback::new(move |_: ()| set_editing.set(false))
                        />
                    }
                    .into_any()
                } else {
                    let item_val = item_sig.get_value();
                    view! {
                        <ReadOnlyDetail item=item_val />
                        <div class="detail-view__actions">
                            <button
                                class="btn btn--primary"
                                on:click=move |_| set_editing.set(true)
                            >
                                "Edit"
                            </button>
                            <button
                                class="btn btn--danger"
                                on:click=move |_| handle_delete()
                            >
                                "Delete"
                            </button>
                        </div>
                    }
                    .into_any()
                }
            }}
        </div>
    }
}

// ─── Read-only display ────────────────────────────────────────────────────────

#[component]
fn ReadOnlyDetail(item: InventoryItem) -> impl IntoView {
    let is_graded = item.kind == "graded";

    view! {
        <dl class="detail-fields">
            <dt>"Type"</dt>
            <dd>{item.kind.clone()}</dd>

            <dt>"Rarity"</dt>
            <dd>{item.rarity.clone()}</dd>

            <dt>"Condition"</dt>
            <dd>{item.condition.clone().unwrap_or_else(|| "—".into())}</dd>

            {(!is_graded).then(|| view! {
                <dt>"Quantity"</dt>
                <dd>{item.quantity}</dd>
            })}

            {is_graded.then(|| view! {
                <dt>"Grade"</dt>
                <dd>{item.grade.clone().unwrap_or_else(|| "—".into())}</dd>
                <dt>"Grader"</dt>
                <dd>{item.grader.clone().unwrap_or_else(|| "—".into())}</dd>
                <dt>"Cert #"</dt>
                <dd>{item.cert_number.clone().unwrap_or_else(|| "—".into())}</dd>
                <dt>"Verification"</dt>
                <dd>{item.verification_status.clone().unwrap_or_else(|| "—".into())}</dd>
            })}

            <dt>"Acquisition cost"</dt>
            <dd>
                {item.acquisition_cost.map(|d| format!("${d}")).unwrap_or_else(|| "—".into())}
            </dd>

            <dt>"Current value"</dt>
            <dd>
                {item.current_value.map(|d| format!("${d}")).unwrap_or_else(|| "—".into())}
            </dd>

            <dt>"Notes"</dt>
            <dd>{item.notes.clone().unwrap_or_else(|| "—".into())}</dd>

            {item.has_risk_flag.then(|| view! {
                <dt>"Risk"</dt>
                <dd><span class="badge badge--risk">"Flagged"</span></dd>
            })}
        </dl>
    }
}

// ─── Edit form ────────────────────────────────────────────────────────────────

#[component]
fn EditForm(
    item: InventoryItem,
    workspace_id: Uuid,
    on_saved: Callback<()>,
    on_conflict: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let is_graded = item.kind == "graded";
    let version: DateTime<Utc> = item.updated_at;
    let id = item.id;

    let (condition, set_condition) = signal(item.condition.clone().unwrap_or_default());
    let (quantity, set_quantity) = signal(item.quantity.to_string());
    let (acq_cost, set_acq_cost) = signal(
        item.acquisition_cost.map(|d| d.to_string()).unwrap_or_default(),
    );
    let (cur_value, set_cur_value) = signal(
        item.current_value.map(|d| d.to_string()).unwrap_or_default(),
    );
    let (notes, set_notes) = signal(item.notes.clone().unwrap_or_default());
    let (err, set_err) = signal(Option::<String>::None);

    let submit = move || {
        let condition_val = condition.get();
        let quantity_val: Option<i64> = if is_graded {
            None
        } else {
            quantity.get().parse().ok()
        };
        let acquisition_cost: Option<Decimal> = acq_cost.get().parse().ok();
        let current_value: Option<Decimal> = cur_value.get().parse().ok();
        let notes_val = notes.get();

        let req = PatchRequest {
            version,
            condition: if condition_val.is_empty() { None } else { Some(condition_val) },
            quantity: quantity_val,
            acquisition_cost,
            notes: if notes_val.is_empty() { None } else { Some(notes_val) },
            current_value,
        };

        leptos::task::spawn_local(async move {
            match api::patch_inventory_item(workspace_id, id, &req).await {
                Ok(_) => on_saved.run(()),
                Err(e) if e == "conflict" => on_conflict.run(()),
                Err(e) => set_err.set(Some(e)),
            }
        });
    };

    view! {
        <form
            class="edit-form"
            on:submit=move |ev| {
                ev.prevent_default();
                submit();
            }
        >
            {is_graded.then(|| view! {
                <p class="edit-form__notice">
                    "Graded card identity (grade, grader, cert #) cannot be changed after certification."
                </p>
            })}

            <label class="field">
                "Condition"
                <select
                    on:change=move |ev| set_condition.set(event_target_value(&ev))
                >
                    <option value="">"—"</option>
                    <option value="mint">"Mint"</option>
                    <option value="near_mint">"Near Mint"</option>
                    <option value="lightly_played">"Lightly Played"</option>
                    <option value="moderately_played">"Moderately Played"</option>
                    <option value="heavily_played">"Heavily Played"</option>
                    <option value="damaged">"Damaged"</option>
                </select>
            </label>

            {(!is_graded).then(|| view! {
                <label class="field">
                    "Quantity"
                    <input
                        type="number"
                        min="0"
                        prop:value=move || quantity.get()
                        on:input=move |ev| set_quantity.set(event_target_value(&ev))
                    />
                </label>
            })}

            <label class="field">
                "Acquisition cost"
                <input
                    type="number"
                    step="0.01"
                    placeholder="0.00"
                    prop:value=move || acq_cost.get()
                    on:input=move |ev| set_acq_cost.set(event_target_value(&ev))
                />
            </label>

            <label class="field">
                "Current value"
                <input
                    type="number"
                    step="0.01"
                    placeholder="0.00"
                    prop:value=move || cur_value.get()
                    on:input=move |ev| set_cur_value.set(event_target_value(&ev))
                />
            </label>

            <label class="field">
                "Notes"
                <textarea
                    prop:value=move || notes.get()
                    on:input=move |ev| set_notes.set(event_target_value(&ev))
                />
            </label>

            {move || err.get().map(|e| view! {
                <p class="edit-form__error">{e}</p>
            })}

            <div class="edit-form__actions">
                <button type="submit" class="btn btn--primary">"Save"</button>
                <button
                    type="button"
                    class="btn"
                    on:click=move |_| on_cancel.run(())
                >
                    "Cancel"
                </button>
            </div>
        </form>
    }
}
