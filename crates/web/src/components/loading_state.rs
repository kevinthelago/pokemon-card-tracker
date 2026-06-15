use leptos::prelude::*;

/// Spinner / skeleton shown while data is loading.
#[component]
pub fn LoadingState(#[prop(optional, into)] message: Option<String>) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center justify-center py-16 gap-3">
            <div class="h-8 w-8 animate-spin rounded-full border-4 border-indigo-600 border-t-transparent" />
            {message.map(|m| view! { <p class="text-sm text-gray-500">{m}</p> })}
        </div>
    }
}
