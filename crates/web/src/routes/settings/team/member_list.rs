//! Member list component: shows each member with their role, and lets owners
//! change roles or remove members. The last-owner constraint is surfaced
//! inline — the action is disabled when the member is the only owner.

use leptos::prelude::*;
use uuid::Uuid;

use crate::api;

use super::{ChangeRoleBody, MemberDto, MemberRole};

#[component]
pub fn MemberList(
    workspace_id: Uuid,
    members: Vec<MemberDto>,
    on_change: Callback<()>,
) -> impl IntoView {
    if members.is_empty() {
        return view! {
            <p class="team-empty">
                "No members yet. Invite your first teammate below."
            </p>
        }
        .into_any();
    }

    let owner_count = members
        .iter()
        .filter(|m| matches!(m.role, MemberRole::Owner))
        .count();

    view! {
        <ul class="member-list">
            {members
                .into_iter()
                .map(|member| {
                    let is_last_owner =
                        matches!(member.role, MemberRole::Owner) && owner_count == 1;
                    view! {
                        <MemberRow
                            workspace_id=workspace_id
                            member=member
                            is_last_owner=is_last_owner
                            on_change=on_change.clone()
                        />
                    }
                })
                .collect_view()}
        </ul>
    }
    .into_any()
}

#[component]
fn MemberRow(
    workspace_id: Uuid,
    member: MemberDto,
    is_last_owner: bool,
    on_change: Callback<()>,
) -> impl IntoView {
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let user_id = member.user_id;
    let name = member.name.clone();
    let email = member.email.clone();
    let current_role = member.role.clone();

    let on_role_change = {
        let on_change = on_change.clone();
        let current_role = current_role.clone();
        move |ev: leptos::ev::Event| {
            let val = event_target_value(&ev);
            let new_role = if val == "owner" { MemberRole::Owner } else { MemberRole::Staff };
            if new_role == current_role {
                return;
            }
            set_busy.set(true);
            set_error.set(None);
            let on_change = on_change.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let body = ChangeRoleBody { role: new_role };
                match api::change_member_role(workspace_id, user_id, &body).await {
                    Ok(_) => on_change.call(()),
                    Err(e) => set_error.set(Some(user_friendly_change_error(&e))),
                }
                set_busy.set(false);
            });
        }
    };

    let on_remove = {
        let on_change = on_change.clone();
        move |_: leptos::ev::MouseEvent| {
            set_busy.set(true);
            set_error.set(None);
            let on_change = on_change.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match api::remove_member(workspace_id, user_id).await {
                    Ok(_) => on_change.call(()),
                    Err(e) => set_error.set(Some(user_friendly_remove_error(&e))),
                }
                set_busy.set(false);
            });
        }
    };

    let is_last_owner_role_title = if is_last_owner {
        "Cannot change the last owner's role"
    } else {
        "Change role"
    };
    let is_last_owner_remove_title = if is_last_owner {
        "Cannot remove the last owner"
    } else {
        "Remove member"
    };
    let owner_selected = matches!(current_role, MemberRole::Owner);
    let staff_selected = matches!(current_role, MemberRole::Staff);

    view! {
        <li class="member-row">
            <div class="member-row__identity">
                <span class="member-row__name">{name}</span>
                <span class="member-row__email text-muted">{email}</span>
            </div>

            <div class="member-row__controls">
                <select
                    class="field__select field__select--inline"
                    disabled=move || busy.get() || is_last_owner
                    title=is_last_owner_role_title
                    on:change=on_role_change
                >
                    <option value="owner" selected=owner_selected>"Owner"</option>
                    <option value="staff" selected=staff_selected>"Staff"</option>
                </select>

                <button
                    class="btn btn--sm btn--danger-ghost"
                    disabled=move || busy.get() || is_last_owner
                    title=is_last_owner_remove_title
                    on:click=on_remove
                >
                    "Remove"
                </button>
            </div>

            {move || {
                error.get().map(|e| {
                    view! { <p class="member-row__error error-text" role="alert">{e}</p> }
                })
            }}
        </li>
    }
}

fn user_friendly_change_error(raw: &str) -> String {
    if raw.contains("last owner") || raw.contains("downgrade") {
        "Cannot change role: this member is the only owner. Promote someone else to owner first."
            .into()
    } else {
        format!("Failed to change role: {raw}")
    }
}

fn user_friendly_remove_error(raw: &str) -> String {
    if raw.contains("last owner") {
        "Cannot remove the last owner of a workspace.".into()
    } else {
        format!("Failed to remove member: {raw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_last_owner_gives_helpful_message() {
        let msg = user_friendly_change_error("403: cannot downgrade the last owner");
        assert!(msg.contains("only owner"));
        assert!(msg.contains("Promote"));
    }

    #[test]
    fn change_unknown_error_falls_through() {
        let msg = user_friendly_change_error("HTTP 500: oops");
        assert!(msg.starts_with("Failed to change role"));
    }

    #[test]
    fn remove_last_owner_gives_helpful_message() {
        let msg = user_friendly_remove_error("403: cannot remove the last owner");
        assert!(msg.contains("last owner"));
    }

    #[test]
    fn remove_unknown_error_falls_through() {
        let msg = user_friendly_remove_error("HTTP 500: oops");
        assert!(msg.starts_with("Failed to remove member"));
    }
}
