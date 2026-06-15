use leptos::prelude::*;

/// Slide-over detail drawer. Used by inventory to show card detail without navigation.
#[component]
pub fn Drawer(
    #[prop(into)] open: Signal<bool>,
    on_close: Callback<()>,
    #[prop(into)] title: String,
    children: ChildrenFn,
) -> impl IntoView {
    // StoredValue makes title Copy so it can be moved into the Fn closure for <Show>.
    let title = StoredValue::new(title);
    view! {
        <Show when=move || open.get()>
            // Backdrop
            <div
                class="fixed inset-0 bg-black/30 z-40 transition-opacity"
                on:click=move |_| on_close.run(())
            />
            // Panel
            <div class="fixed inset-y-0 right-0 z-50 flex w-full max-w-md flex-col bg-white shadow-xl">
                <div class="flex items-center justify-between border-b border-gray-200 px-5 py-4">
                    <h2 class="text-base font-semibold text-gray-900">{title.get_value()}</h2>
                    <button
                        class="rounded-md p-1 text-gray-400 hover:text-gray-600"
                        on:click=move |_| on_close.run(())
                        aria-label="Close"
                    >
                        <svg class="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                            <path d="M6.28 5.22a.75.75 0 00-1.06 1.06L8.94 10l-3.72 3.72a.75.75 \
                                     0 101.06 1.06L10 11.06l3.72 3.72a.75.75 0 101.06-1.06L11.06 \
                                     10l3.72-3.72a.75.75 0 00-1.06-1.06L10 8.94 6.28 5.22z" />
                        </svg>
                    </button>
                </div>
                <div class="flex-1 overflow-y-auto px-5 py-4">
                    {children()}
                </div>
            </div>
        </Show>
    }
}
