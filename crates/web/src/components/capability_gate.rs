use leptos::prelude::*;

/// Hides `children` when the active workspace does not have the required capability.
///
/// Seller-only features (POS, fraud, reconciliation) use this gate.
/// The feature streams provide the workspace kind via context.
#[component]
pub fn CapabilityGate(
    /// When false the children are hidden entirely (not just disabled).
    #[prop(into)]
    enabled: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <Show when=move || enabled.get()>
            {children()}
        </Show>
    }
}
