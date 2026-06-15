use leptos::prelude::*;

/// Shown when a network request or action fails.
#[component]
pub fn ErrorState(
    #[prop(into)] message: String,
    #[prop(optional)] retry: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="rounded-md bg-red-50 border border-red-200 p-4 flex gap-3">
            <svg class="h-5 w-5 text-red-400 flex-shrink-0 mt-0.5" viewBox="0 0 20 20" fill="currentColor">
                <path fill-rule="evenodd"
                    d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.28 7.22a.75.75 0 00-1.06 \
                       1.06L8.94 10l-1.72 1.72a.75.75 0 101.06 1.06L10 11.06l1.72 1.72a.75.75 \
                       0 101.06-1.06L11.06 10l1.72-1.72a.75.75 0 00-1.06-1.06L10 8.94 8.28 7.22z"
                    clip-rule="evenodd" />
            </svg>
            <div class="flex-1">
                <p class="text-sm text-red-700">{message}</p>
                {retry.map(|cb| view! {
                    <button
                        class="mt-2 text-sm font-medium text-red-700 underline"
                        on:click=move |_| cb.run(())
                    >
                        "Try again"
                    </button>
                })}
            </div>
        </div>
    }
}
