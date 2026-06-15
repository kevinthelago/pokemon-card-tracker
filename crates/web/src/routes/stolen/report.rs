//! Stolen-card report submission form (CSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api;

#[component]
pub fn ReportForm() -> impl IntoView {
    let grader = RwSignal::new(String::new());
    let cert_number = RwSignal::new(String::new());
    let evidence = RwSignal::new(String::new());
    let notes = RwSignal::new(String::new());

    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (success_id, set_success_id) = signal(Option::<String>::None);

    let handle_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_busy.set(true);
        set_error.set(None);

        let g = grader.get();
        let c = cert_number.get();
        let ev_val = evidence.get();
        let n = notes.get();

        wasm_bindgen_futures::spawn_local(async move {
            match api::submit_stolen_report(
                g,
                c,
                (!ev_val.is_empty()).then_some(ev_val),
                (!n.is_empty()).then_some(n),
            )
            .await
            {
                Ok(id) => set_success_id.set(Some(id)),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    view! {
        <div class="max-w-2xl mx-auto p-6">
            <h1 class="text-2xl font-bold text-gray-900 mb-2">"Report a Stolen Card"</h1>
            <p class="text-sm text-gray-600 mb-6">
                "Reports for graded cards are cert-based and reliable. \
                 Raw (ungraded) cards have no serial number and cannot be reliably matched in v1."
            </p>

            {move || success_id.get().map(|_| view! {
                <div class="rounded-md bg-green-50 border border-green-200 p-4 mb-6">
                    <p class="text-green-800 font-medium">"Report submitted successfully."</p>
                    <p class="text-green-700 text-sm mt-1">
                        "Your report is now in the moderation queue. You will be notified of any updates."
                    </p>
                    <A href="/stolen/my-reports" attr:class="mt-2 inline-block text-sm text-green-700 underline">
                        "View my reports →"
                    </A>
                </div>
            })}

            {move || error.get().map(|e| view! {
                <div class="rounded-md bg-red-50 border border-red-200 p-4 mb-6">
                    <p class="text-red-800 font-medium">"Submission failed"</p>
                    <p class="text-red-700 text-sm mt-1">{e}</p>
                </div>
            })}

            {move || success_id.get().is_none().then(|| view! {
                <form on:submit=handle_submit>
                    <div class="mb-4">
                        <label for="grader" class="block text-sm font-medium text-gray-700 mb-1">
                            "Grader" <span class="text-red-500">"*"</span>
                        </label>
                        <select
                            id="grader"
                            required
                            class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                            on:change=move |e| grader.set(event_target_value(&e))
                        >
                            <option value="">"Select grader…"</option>
                            <option value="PSA">"PSA"</option>
                            <option value="CGC">"CGC"</option>
                            <option value="BGS">"BGS"</option>
                        </select>
                    </div>

                    <div class="mb-4">
                        <label for="cert_number" class="block text-sm font-medium text-gray-700 mb-1">
                            "Certification number" <span class="text-red-500">"*"</span>
                        </label>
                        <input
                            id="cert_number"
                            type="text"
                            required
                            placeholder="e.g. 12345678"
                            class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                            prop:value=move || cert_number.get()
                            on:input=move |e| cert_number.set(event_target_value(&e))
                        />
                        <p class="mt-1 text-xs text-gray-500">
                            "Found on the cert label or the grading service website."
                        </p>
                    </div>

                    <div class="mb-4">
                        <label for="evidence" class="block text-sm font-medium text-gray-700 mb-1">
                            "Evidence / description"
                        </label>
                        <textarea
                            id="evidence"
                            rows="3"
                            placeholder="Describe how you know this card was stolen (police report #, purchase records, etc.)"
                            class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                            prop:value=move || evidence.get()
                            on:input=move |e| evidence.set(event_target_value(&e))
                        />
                    </div>

                    <div class="mb-6">
                        <label for="notes" class="block text-sm font-medium text-gray-700 mb-1">
                            "Additional notes"
                        </label>
                        <textarea
                            id="notes"
                            rows="2"
                            placeholder="Any other context for the moderator"
                            class="w-full border border-gray-300 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                            prop:value=move || notes.get()
                            on:input=move |e| notes.set(event_target_value(&e))
                        />
                    </div>

                    <div class="rounded-md bg-blue-50 border border-blue-200 p-3 mb-4 text-sm text-blue-800">
                        "Community reports are reviewed by platform moderators before being added to the shared list. \
                         Only the cert# and grader are published — your identity and evidence are never shared publicly."
                    </div>

                    <div class="flex gap-3 items-center">
                        <button
                            type="submit"
                            disabled=move || busy.get()
                            class="bg-blue-600 hover:bg-blue-700 disabled:bg-blue-400 text-white font-medium px-4 py-2 rounded-md text-sm transition-colors"
                        >
                            {move || if busy.get() { "Submitting…" } else { "Submit report" }}
                        </button>
                        <A href="/stolen/my-reports" attr:class="text-sm text-gray-600 hover:underline">
                            "View my reports"
                        </A>
                    </div>
                </form>
            })}
        </div>
    }
}
