use leptos::prelude::*;

/// Shown when a list or page has no content yet.
#[component]
pub fn EmptyState(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    #[prop(optional)] action: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center justify-center py-16 text-center">
            <div class="rounded-full bg-gray-100 p-4 mb-4">
                <svg class="h-8 w-8 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                        d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 \
                           2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 \
                           2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 \
                           1 0 006.586 13H4" />
                </svg>
            </div>
            <h3 class="text-base font-semibold text-gray-900">{title}</h3>
            <p class="mt-1 text-sm text-gray-500 max-w-sm">{description}</p>
            {action.map(|a| view! { <div class="mt-4">{a}</div> })}
        </div>
    }
}
