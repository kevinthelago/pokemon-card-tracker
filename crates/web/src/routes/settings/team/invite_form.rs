//! Invite-by-email form component.
//!
//! Emits `on_sent` after a successful invite so the parent page reloads the
//! team list. Prevents duplicate submission while the request is in flight.

use leptos::prelude::*;
use uuid::Uuid;

use crate::api;

use super::{MemberRole, SendInviteBody};

#[component]
pub fn InviteForm(workspace_id: Uuid, on_sent: Callback<()>) -> impl IntoView {
    let (email, set_email) = signal(String::new());
    let (role, set_role) = signal(MemberRole::Staff);
    let (submitting, set_submitting) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    // Callback<T> is Clone but not Copy — clone per call so the handler stays Fn.
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if submitting.get() {
            return;
        }
        let email_val = email.get().trim().to_string();
        if email_val.is_empty() {
            set_error.set(Some("Email is required.".into()));
            return;
        }

        set_submitting.set(true);
        set_error.set(None);

        let body = SendInviteBody {
            email: email_val,
            role: role.get(),
        };
        let on_sent = on_sent.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match api::send_invite(workspace_id, &body).await {
                Ok(_) => {
                    set_email.set(String::new());
                    set_role.set(MemberRole::Staff);
                    on_sent.run(());
                }
                Err(e) => {
                    set_error.set(Some(user_friendly_error(&e)));
                }
            }
            set_submitting.set(false);
        });
    };

    view! {
        <form class="invite-form" on:submit=on_submit>
            <div class="invite-form__fields">
                <div class="field">
                    <label class="field__label" for="invite-email">"Email address"</label>
                    <input
                        id="invite-email"
                        class="field__input"
                        type="email"
                        placeholder="teammate@example.com"
                        required
                        autocomplete="email"
                        prop:value=move || email.get()
                        on:input=move |ev| set_email.set(event_target_value(&ev))
                        disabled=move || submitting.get()
                    />
                </div>

                <div class="field">
                    <label class="field__label" for="invite-role">"Role"</label>
                    <select
                        id="invite-role"
                        class="field__select"
                        on:change=move |ev| {
                            let val = event_target_value(&ev);
                            set_role.set(if val == "owner" {
                                MemberRole::Owner
                            } else {
                                MemberRole::Staff
                            });
                        }
                        disabled=move || submitting.get()
                    >
                        <option value="staff" selected=move || matches!(role.get(), MemberRole::Staff)>
                            "Staff \u{2014} operates catalogue and triages flags"
                        </option>
                        <option value="owner" selected=move || matches!(role.get(), MemberRole::Owner)>
                            "Owner \u{2014} full access including team and settings"
                        </option>
                    </select>
                </div>
            </div>

            {move || error.get().map(|e| view! {
                <p class="invite-form__error error-text" role="alert">{e}</p>
            })}

            <button
                type="submit"
                class="btn btn--primary"
                disabled=move || submitting.get()
            >
                {move || if submitting.get() { "Sending\u{2026}" } else { "Send invitation" }}
            </button>
        </form>
    }
}

fn user_friendly_error(raw: &str) -> String {
    if raw.contains("already a member") {
        "This person is already a member of the workspace.".into()
    } else if raw.contains("pending invite already exists") {
        "A pending invitation already exists for this email. Use Resend to re-deliver it.".into()
    } else {
        format!("Failed to send invitation: {raw}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_member_error_is_friendly() {
        let msg = user_friendly_error("HTTP 409: already a member");
        assert!(msg.contains("already a member"));
        assert!(!msg.starts_with("Failed"));
    }

    #[test]
    fn duplicate_invite_error_is_friendly() {
        let msg = user_friendly_error("HTTP 409: pending invite already exists");
        assert!(msg.contains("pending invitation"));
        assert!(msg.contains("Resend"));
    }

    #[test]
    fn unknown_error_falls_through() {
        let msg = user_friendly_error("HTTP 500: server exploded");
        assert!(msg.starts_with("Failed to send invitation"));
        assert!(msg.contains("server exploded"));
    }
}
