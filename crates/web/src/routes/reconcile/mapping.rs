use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};
use uuid::Uuid;

use crate::api;
use super::{MappingDto, MappingQueueDto, UnmappedSkuDto};

// ─── UnmappedSkuRow component ─────────────────────────────────────────────────

#[component]
fn UnmappedSkuRow(
    sku: UnmappedSkuDto,
    workspace_id: Uuid,
    on_mapped: Callback<()>,
) -> impl IntoView {
    let pos_sku = sku.pos_sku.clone();
    let product_name = sku.pos_product_name.clone().unwrap_or_else(|| "\u{2014}".into());
    let last_seen = sku.last_seen_at.clone();
    let connection_id = sku.connection_id;

    let (printing_id_input, set_printing_id) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (success_msg, set_success_msg) = signal(Option::<String>::None);

    let sku_for_submit = pos_sku.clone();
    let submit = move |_| {
        let pid_str = printing_id_input.get_untracked();
        let Ok(pid) = Uuid::parse_str(&pid_str) else {
            set_error.set(Some("Invalid Printing ID (must be a UUID)".into()));
            return;
        };
        set_busy.set(true);
        set_error.set(None);
        let sku = sku_for_submit.clone();
        let on_mapped = on_mapped.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api::create_mapping(workspace_id, connection_id, sku.clone(), pid).await {
                Ok(result) => {
                    set_success_msg.set(Some(format!(
                        "\u{2713} Mapped \u{2014} {} historical sale{} reconciled",
                        result.backfilled_lines,
                        if result.backfilled_lines == 1 { "" } else { "s" }
                    )));
                    on_mapped.run(());
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    view! {
        <tr class="unmapped-row">
            <td class="sku-cell"><code>{pos_sku}</code></td>
            <td>{product_name}</td>
            <td class="date-cell">{last_seen}</td>
            <td>
                // Show success message or the mapping form; toggled via CSS so
                // `submit` doesn't need to live inside a re-invoked reactive closure.
                <span
                    class="backfill-note"
                    style=move || if success_msg.get().is_some() { "" } else { "display:none" }
                >
                    {move || success_msg.get().unwrap_or_default()}
                </span>
                <div
                    class="map-form"
                    style=move || if success_msg.get().is_none() { "" } else { "display:none" }
                >
                    <input
                        type="text"
                        class="printing-id-input"
                        placeholder="Printing ID (UUID)"
                        on:input=move |ev| set_printing_id.set(event_target_value(&ev))
                        prop:disabled=move || busy.get()
                    />
                    <button
                        class="btn btn-sm btn-primary"
                        disabled=move || printing_id_input.get().is_empty() || busy.get()
                        on:click=submit
                    >
                        {move || if busy.get() { "Mapping\u{2026}" } else { "Map" }}
                    </button>
                    {move || error.get().map(|e| view! {
                        <span class="error-text">{e}</span>
                    })}
                </div>
            </td>
        </tr>
    }
}

// ─── ExistingMappingRow component ─────────────────────────────────────────────

#[component]
fn ExistingMappingRow(
    mapping: MappingDto,
    workspace_id: Uuid,
    on_deleted: Callback<()>,
) -> impl IntoView {
    let mid = mapping.id;
    let pos_sku = mapping.pos_sku.clone();
    let printing_id = mapping.printing_id.to_string();
    let created_at = mapping.created_at.clone();

    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let on_delete = move |_| {
        set_busy.set(true);
        set_error.set(None);
        let on_deleted = on_deleted.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match api::delete_mapping(workspace_id, mid).await {
                Ok(_) => on_deleted.run(()),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    view! {
        <tr>
            <td class="sku-cell"><code>{pos_sku}</code></td>
            <td class="uuid-cell">{printing_id}</td>
            <td class="date-cell">{created_at}</td>
            <td>
                <button
                    class="btn btn-sm btn-ghost btn-danger"
                    disabled=move || busy.get()
                    on:click=on_delete
                >
                    "Remove"
                </button>
                {move || error.get().map(|e| view! {
                    <span class="error-text">{e}</span>
                })}
            </td>
        </tr>
    }
}

// ─── MappingQueuePage component ───────────────────────────────────────────────

#[component]
pub fn MappingQueuePage() -> impl IntoView {
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
    let bump_reload = move || set_reload.update(|n| *n += 1);

    let queue_data = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move { api::fetch_mapping_queue(wid).await.ok() }
    });

    let back_href = move || format!("/workspaces/{}/reconcile", workspace_id());

    view! {
        <div class="mapping-queue-page">
            <div class="page-header">
                <A href=back_href>"\u{2190} Back to Dashboard"</A>
                <h1>"SKU \u{2192} Printing Mapping"</h1>
            </div>

            <Suspense fallback=|| view! { <p class="loading">"Loading\u{2026}"</p> }>
                {move || {
                    match queue_data.get().map(|sw| sw.take()) {
                        None => view! { <p class="loading">"Loading\u{2026}"</p> }.into_any(),
                        Some(None) => view! {
                            <div class="error-banner">
                                <p>"Failed to load mapping queue."</p>
                            </div>
                        }.into_any(),
                        Some(Some(MappingQueueDto { unmapped, mappings })) => {
                            let wid = workspace_id();
                            let bump1 = bump_reload.clone();
                            let bump2 = bump_reload.clone();
                            view! {
                                <div>
                                    // Unmapped queue section
                                    <section class="queue-section">
                                        <h2>
                                            "Unmapped SKUs "
                                            {if unmapped.is_empty() {
                                                view! { <span class="badge badge-success">"All mapped"</span> }.into_any()
                                            } else {
                                                view! {
                                                    <span class="badge badge-warn">
                                                        {format!("{} pending", unmapped.len())}
                                                    </span>
                                                }.into_any()
                                            }}
                                        </h2>

                                        {if unmapped.is_empty() {
                                            view! {
                                                <p class="empty-state">
                                                    "No unmapped SKUs \u{2014} all POS products are linked to printings."
                                                </p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <p class="queue-hint">
                                                    "Enter the Printing ID (UUID) for each SKU. \
                                                     Historical sales will be reconciled automatically."
                                                </p>
                                                <table class="mapping-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"POS SKU"</th>
                                                            <th>"POS Product Name"</th>
                                                            <th>"Last Seen"</th>
                                                            <th>"Map to Printing"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {unmapped.into_iter().map(|sku| {
                                                            let b = bump1.clone();
                                                            view! {
                                                                <UnmappedSkuRow
                                                                    sku=sku
                                                                    workspace_id=wid
                                                                    on_mapped=Callback::new(move |_: ()| b())
                                                                />
                                                            }
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            }.into_any()
                                        }}
                                    </section>

                                    // Existing mappings section
                                    <section class="mappings-section">
                                        <h2>"Existing Mappings"</h2>

                                        {if mappings.is_empty() {
                                            view! {
                                                <p class="empty-state">"No mappings yet."</p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <table class="mapping-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"POS SKU"</th>
                                                            <th>"Printing ID"</th>
                                                            <th>"Created"</th>
                                                            <th></th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {mappings.into_iter().map(|mapping| {
                                                            let b = bump2.clone();
                                                            view! {
                                                                <ExistingMappingRow
                                                                    mapping=mapping
                                                                    workspace_id=wid
                                                                    on_deleted=Callback::new(move |_: ()| b())
                                                                />
                                                            }
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            }.into_any()
                                        }}
                                    </section>
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </Suspense>
        </div>
    }
}
