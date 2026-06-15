use leptos::*;
use leptos_router::A;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn sf(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::ServerError(e.to_string())
}

// ─── Shared types (mirrored from cardguard-api) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingView {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub printing_id: Uuid,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnmappedSkuView {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub pos_sku: String,
    pub pos_product_name: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingQueueData {
    pub mappings: Vec<MappingView>,
    pub unmapped: Vec<UnmappedSkuView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMappingResult {
    pub mapping: MappingView,
    pub backfilled_lines: i64,
}

// ─── Server functions ─────────────────────────────────────────────────────────

#[server(GetMappingQueue, "/api")]
pub async fn get_mapping_queue(
    workspace_id: String,
    connection_id: Option<String>,
) -> Result<MappingQueueData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use cardguard_api::MappingService;
        use leptos_axum::extract;
        use sqlx::PgPool;

        let pool = extract::<axum::Extension<PgPool>>()
            .await
            .map(|e| e.0)
            .map_err(|e| sf(e))?;

        let wid = Uuid::parse_str(&workspace_id)
            .map_err(|_| sf("Invalid workspace ID"))?;

        let cid = connection_id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| sf("Invalid connection ID"))?;

        let mappings = MappingService::list_mappings(&pool, wid, cid)
            .await
            .map_err(|e| sf(e))?;

        let unmapped = MappingService::list_unmapped(&pool, wid, cid)
            .await
            .map_err(|e| sf(e))?;

        Ok(MappingQueueData {
            mappings: mappings
                .into_iter()
                .map(|m| MappingView {
                    id: m.id,
                    connection_id: m.connection_id,
                    pos_sku: m.pos_sku,
                    printing_id: m.printing_id,
                    created_at: m.created_at.to_rfc3339(),
                })
                .collect(),
            unmapped: unmapped
                .into_iter()
                .map(|u| UnmappedSkuView {
                    id: u.id,
                    connection_id: u.connection_id,
                    pos_sku: u.pos_sku,
                    pos_product_name: u.pos_product_name,
                    last_seen_at: u.last_seen_at.to_rfc3339(),
                })
                .collect(),
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(sf("SSR only"))
    }
}

#[server(CreateSkuMapping, "/api")]
pub async fn create_sku_mapping(
    workspace_id: String,
    connection_id: String,
    pos_sku: String,
    printing_id: String,
) -> Result<CreateMappingResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use cardguard_api::MappingService;
        use leptos_axum::extract;
        use sqlx::PgPool;

        let pool = extract::<axum::Extension<PgPool>>()
            .await
            .map(|e| e.0)
            .map_err(|e| sf(e))?;

        let wid = Uuid::parse_str(&workspace_id)
            .map_err(|_| sf("Invalid workspace ID"))?;
        let cid = Uuid::parse_str(&connection_id)
            .map_err(|_| sf("Invalid connection ID"))?;
        let pid = Uuid::parse_str(&printing_id)
            .map_err(|_| sf("Invalid printing ID"))?;

        let result = MappingService::create_mapping(&pool, wid, cid, &pos_sku, pid)
            .await
            .map_err(|e| sf(e))?;

        Ok(CreateMappingResult {
            mapping: MappingView {
                id: result.mapping.id,
                connection_id: result.mapping.connection_id,
                pos_sku: result.mapping.pos_sku,
                printing_id: result.mapping.printing_id,
                created_at: result.mapping.created_at.to_rfc3339(),
            },
            backfilled_lines: result.backfilled_lines,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(sf("SSR only"))
    }
}

#[server(DeleteSkuMapping, "/api")]
pub async fn delete_sku_mapping(
    workspace_id: String,
    mapping_id: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use cardguard_api::MappingService;
        use leptos_axum::extract;
        use sqlx::PgPool;

        let pool = extract::<axum::Extension<PgPool>>()
            .await
            .map(|e| e.0)
            .map_err(|e| sf(e))?;

        let wid = Uuid::parse_str(&workspace_id)
            .map_err(|_| sf("Invalid workspace ID"))?;
        let mid = Uuid::parse_str(&mapping_id)
            .map_err(|_| sf("Invalid mapping ID"))?;

        MappingService::delete_mapping(&pool, wid, mid)
            .await
            .map_err(|e| sf(e))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(sf("SSR only"))
    }
}

// ─── UnmappedSkuRow component ─────────────────────────────────────────────────

#[component]
fn UnmappedSkuRow(
    sku: UnmappedSkuView,
    workspace_id: String,
    on_mapped: Action<(String, String, String, String), Result<CreateMappingResult, ServerFnError>>,
) -> impl IntoView {
    // Pre-extract fields before any closures capture them.
    let pos_sku = sku.pos_sku.clone();
    let product_name = sku.pos_product_name.clone().unwrap_or_else(|| "—".into());
    let last_seen = sku.last_seen_at.clone();
    let connection_id_str = sku.connection_id.to_string();

    let (printing_id_input, set_printing_id) = create_signal(String::new());
    let (is_submitted, set_submitted) = create_signal(false);

    let pos_sku_for_submit = pos_sku.clone();
    let wid_for_submit = workspace_id.clone();
    let cid_for_submit = connection_id_str.clone();
    let submit = move |_| {
        let pid = printing_id_input.get();
        if pid.is_empty() {
            return;
        }
        set_submitted.set(true);
        on_mapped.dispatch((
            wid_for_submit.clone(),
            cid_for_submit.clone(),
            pos_sku_for_submit.clone(),
            pid,
        ));
    };

    let pos_sku_for_effect = pos_sku.clone();
    create_effect(move |_| {
        if let Some(Ok(result)) = on_mapped.value().get() {
            if result.mapping.pos_sku == pos_sku_for_effect {
                tracing::info!(
                    sku = %pos_sku_for_effect,
                    backfilled = result.backfilled_lines,
                    "SKU mapped successfully",
                );
            }
        }
    });

    let pos_sku_for_view = pos_sku.clone();
    view! {
        <tr class="unmapped-row">
            <td class="sku-cell">
                <code>{pos_sku.clone()}</code>
            </td>
            <td>{product_name}</td>
            <td class="date-cell">{last_seen}</td>
            <td>
                <div class="map-form">
                    <input
                        type="text"
                        class="printing-id-input"
                        placeholder="Printing ID (UUID)"
                        on:input=move |ev| set_printing_id.set(event_target_value(&ev))
                        prop:value=printing_id_input
                        prop:disabled=is_submitted
                    />
                    <button
                        class="btn btn-sm btn-primary"
                        on:click=submit
                        prop:disabled=move || printing_id_input.get().is_empty() || is_submitted.get()
                    >
                        {move || if is_submitted.get() { "Mapping…" } else { "Map" }}
                    </button>
                </div>
                {move || {
                    on_mapped.value().get()
                        .and_then(|r| r.ok())
                        .filter(|r| r.mapping.pos_sku == pos_sku_for_view)
                        .map(|r| view! {
                            <span class="backfill-note">
                                {format!(
                                    "✓ Mapped — {} historical sale{} reconciled",
                                    r.backfilled_lines,
                                    if r.backfilled_lines == 1 { "" } else { "s" }
                                )}
                            </span>
                        })
                }}
            </td>
        </tr>
    }
}

// ─── ExistingMappingRow component ─────────────────────────────────────────────

#[component]
fn ExistingMappingRow(
    mapping: MappingView,
    workspace_id: String,
    on_deleted: Action<(String, String), Result<(), ServerFnError>>,
) -> impl IntoView {
    let mid = mapping.id.to_string();
    let wid = workspace_id;
    let pos_sku = mapping.pos_sku.clone();
    let printing_id = mapping.printing_id.to_string();
    let created_at = mapping.created_at.clone();

    view! {
        <tr>
            <td class="sku-cell"><code>{pos_sku}</code></td>
            <td class="uuid-cell">{printing_id}</td>
            <td class="date-cell">{created_at}</td>
            <td>
                <button
                    class="btn btn-sm btn-ghost btn-danger"
                    on:click=move |_| {
                        on_deleted.dispatch((wid.clone(), mid.clone()));
                    }
                >
                    "Remove"
                </button>
            </td>
        </tr>
    }
}

// ─── MappingQueuePage component ───────────────────────────────────────────────

#[component]
pub fn MappingQueuePage() -> impl IntoView {
    // &'static str is Copy — freely captured by multiple closures.
    // Auth stream wires in the real workspace ID.
    const WID: &str = "00000000-0000-0000-0000-000000000000";

    let (refresh, set_refresh) = create_signal(0u32);

    let queue_data = create_resource(
        move || (WID.to_string(), refresh.get()),
        |(wid, _)| async move { get_mapping_queue(wid, None).await },
    );

    let create_mapping_action = create_action(
        |(wid, cid, sku, pid): &(String, String, String, String)| {
            let wid = wid.clone();
            let cid = cid.clone();
            let sku = sku.clone();
            let pid = pid.clone();
            async move { create_sku_mapping(wid, cid, sku, pid).await }
        },
    );

    let delete_action = create_action(|(wid, mid): &(String, String)| {
        let wid = wid.clone();
        let mid = mid.clone();
        async move { delete_sku_mapping(wid, mid).await }
    });

    // Refresh the list after create or delete succeeds.
    create_effect(move |_| {
        let created = create_mapping_action.value().get().map(|r| r.is_ok()).unwrap_or(false);
        let deleted = delete_action.value().get().map(|r| r.is_ok()).unwrap_or(false);
        if created || deleted {
            set_refresh.update(|n| *n += 1);
        }
    });

    view! {
        <div class="mapping-queue-page">
            <div class="page-header">
                <A href="/reconcile">"← Back to Dashboard"</A>
                <h1>"SKU → Printing Mapping"</h1>
            </div>

            <Suspense fallback=|| view! { <p class="loading">"Loading…"</p> }>
                {move || {
                    queue_data.get().map(|res| match res {
                        Err(e) => view! {
                            <div class="error-banner">
                                <p>"Failed to load mapping queue: " {e.to_string()}</p>
                            </div>
                        }.into_view(),
                        Ok(data) => {
                            let unmapped = data.unmapped.clone();
                            let mappings = data.mappings.clone();

                            view! {
                                <div>
                                    // Unmapped queue section
                                    <section class="queue-section">
                                        <h2>
                                            "Unmapped SKUs "
                                            {if unmapped.is_empty() {
                                                view! { <span class="badge badge-success">"All mapped"</span> }.into_view()
                                            } else {
                                                view! {
                                                    <span class="badge badge-warn">
                                                        {format!("{} pending", unmapped.len())}
                                                    </span>
                                                }.into_view()
                                            }}
                                        </h2>

                                        {if unmapped.is_empty() {
                                            view! {
                                                <p class="empty-state">
                                                    "No unmapped SKUs — all POS products are linked to printings."
                                                </p>
                                            }.into_view()
                                        } else {
                                            let create_action_clone = create_mapping_action.clone();
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
                                                        <For
                                                            each=move || unmapped.clone()
                                                            key=|u| u.id
                                                            children=move |sku| {
                                                                view! {
                                                                    <UnmappedSkuRow
                                                                        sku=sku
                                                                        workspace_id=WID.to_string()
                                                                        on_mapped=create_action_clone.clone()
                                                                    />
                                                                }
                                                            }
                                                        />
                                                    </tbody>
                                                </table>
                                            }.into_view()
                                        }}
                                    </section>

                                    // Existing mappings section
                                    <section class="mappings-section">
                                        <h2>"Existing Mappings"</h2>

                                        {if mappings.is_empty() {
                                            view! {
                                                <p class="empty-state">"No mappings yet."</p>
                                            }.into_view()
                                        } else {
                                            let delete_action_clone = delete_action.clone();
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
                                                        <For
                                                            each=move || mappings.clone()
                                                            key=|m| m.id
                                                            children=move |mapping| {
                                                                view! {
                                                                    <ExistingMappingRow
                                                                        mapping=mapping
                                                                        workspace_id=WID.to_string()
                                                                        on_deleted=delete_action_clone.clone()
                                                                    />
                                                                }
                                                            }
                                                        />
                                                    </tbody>
                                                </table>
                                            }.into_view()
                                        }}
                                    </section>
                                </div>
                            }.into_view()
                        }
                    })
                }}
            </Suspense>

            // Global error toast for delete failures.
            {move || {
                delete_action.value().get()
                    .and_then(|r| r.err())
                    .map(|e| view! {
                        <div class="toast toast-error">
                            "Delete failed: " {e.to_string()}
                        </div>
                    })
            }}
        </div>
    }
}
