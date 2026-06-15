//! Top-level team settings page.
//!
//! Reads `:wid` from the URL, fetches team data, and delegates to
//! `MemberList` and `InviteForm`. Hidden for collector workspaces — the
//! router only mounts this page on the seller-scoped settings route.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use crate::api;

use super::{invite_form::InviteForm, member_list::MemberList, InviteDto, TeamResponse};

#[component]
pub fn TeamPage() -> impl IntoView {
    let params = use_params_map();

    let workspace_id = move || {
        params.with(|p| {
            p.get("wid")
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok())
        })
    };

    // Bump to force a refetch after any mutation.
    let (reload, set_reload) = signal(0u32);

    let team = LocalResource::new(move || {
        let wid = workspace_id();
        let _ = reload.get();
        async move {
            let wid = wid?;
            api::fetch_team(wid).await.ok()
        }
    });

    let trigger_reload = move || set_reload.update(|n| *n += 1);

    view! {
        <div class="team-page">
            <h1 class="team-page__title">"Team"</h1>
            <p class="team-page__subtitle">"Manage who has access to this workspace."</p>

            <Suspense fallback=move || view! { <p class="loading">"Loading team\u{2026}"</p> }>
                {move || {
                    match team.get().as_deref() {
                        None => view! { <p class="loading">"Loading\u{2026}"</p> }.into_any(),
                        Some(None) => {
                            view! {
                                <p class="error">
                                    "Failed to load team data. Please refresh."
                                </p>
                            }
                            .into_any()
                        }
                        Some(Some(data)) => {
                            let wid = workspace_id().unwrap_or_default();
                            let on_change = Callback::new(move |_: ()| trigger_reload());
                            view! {
                                <TeamContent workspace_id=wid data=data.clone() on_change=on_change />
                            }
                            .into_any()
                        }
                    }
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn TeamContent(workspace_id: Uuid, data: TeamResponse, on_change: Callback<()>) -> impl IntoView {
    view! {
        <section class="team-section">
            <h2 class="team-section__heading">"Members"</h2>
            <MemberList
                workspace_id=workspace_id
                members=data.members.clone()
                on_change=on_change.clone()
            />
        </section>

        <section class="team-section">
            <h2 class="team-section__heading">"Pending invitations"</h2>

            {if data.pending_invites.is_empty() {
                view! { <p class="team-empty">"No pending invitations."</p> }.into_any()
            } else {
                view! {
                    <ul class="invite-list">
                        {data
                            .pending_invites
                            .into_iter()
                            .map(|inv| {
                                view! {
                                    <PendingInviteRow
                                        workspace_id=workspace_id
                                        invite=inv
                                        on_change=on_change.clone()
                                    />
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                .into_any()
            }}
        </section>

        <section class="team-section">
            <h2 class="team-section__heading">"Invite a teammate"</h2>
            <InviteForm workspace_id=workspace_id on_sent=on_change />
        </section>
    }
}

#[component]
fn PendingInviteRow(
    workspace_id: Uuid,
    invite: InviteDto,
    on_change: Callback<()>,
) -> impl IntoView {
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let invite_id = invite.id;
    let email = invite.email.clone();
    let role_label = invite.role.label();

    let on_resend = {
        let on_change = on_change.clone();
        move |_: leptos::ev::MouseEvent| {
            set_busy.set(true);
            set_error.set(None);
            let on_change = on_change.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api::resend_invite(workspace_id, invite_id).await {
                    Ok(_) => on_change.run(()),
                    Err(e) => set_error.set(Some(format!("Failed to resend: {e}"))),
                }
                set_busy.set(false);
            });
        }
    };

    let on_cancel = {
        let on_change = on_change.clone();
        move |_: leptos::ev::MouseEvent| {
            set_busy.set(true);
            set_error.set(None);
            let on_change = on_change.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api::cancel_invite(workspace_id, invite_id).await {
                    Ok(_) => on_change.run(()),
                    Err(e) => set_error.set(Some(format!("Failed to cancel: {e}"))),
                }
                set_busy.set(false);
            });
        }
    };

    view! {
        <li class="invite-row">
            <span class="invite-row__email">{email.clone()}</span>
            <span class="invite-row__role badge">{role_label}</span>
            <span class="invite-row__status">"Pending"</span>

            {move || {
                error
                    .get()
                    .map(|e| view! { <span class="invite-row__error error-text">{e}</span> })
            }}

            <div class="invite-row__actions">
                <button
                    class="btn btn--sm btn--ghost"
                    disabled=move || busy.get()
                    on:click=on_resend
                >
                    "Resend"
                </button>
                <button
                    class="btn btn--sm btn--danger-ghost"
                    disabled=move || busy.get()
                    on:click=on_cancel
                >
                    "Cancel"
                </button>
            </div>
        </li>
    }
}
