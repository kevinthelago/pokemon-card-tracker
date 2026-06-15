use leptos::prelude::*;

/// Multi-step wizard shell for flows like CSV import and card cataloguing.
#[component]
pub fn Wizard(
    steps: Vec<WizardStep>,
    /// 0-based index of the currently active step.
    current: ReadSignal<usize>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="space-y-6">
            // Step progress indicator
            <nav aria-label="Progress">
                <ol class="flex items-center gap-2">
                    {steps.into_iter().enumerate().map(|(i, step)| {
                        let is_complete = move || i < current.get();
                        let is_active = move || i == current.get();
                        view! {
                            <li class="flex items-center gap-2">
                                <span class=move || {
                                    if is_complete() {
                                        "h-6 w-6 rounded-full bg-indigo-600 flex items-center justify-center text-white text-xs font-bold"
                                    } else if is_active() {
                                        "h-6 w-6 rounded-full border-2 border-indigo-600 flex items-center justify-center text-indigo-600 text-xs font-bold"
                                    } else {
                                        "h-6 w-6 rounded-full border-2 border-gray-300 flex items-center justify-center text-gray-400 text-xs"
                                    }
                                }>
                                    {i + 1}
                                </span>
                                <span class=move || {
                                    if is_active() { "text-sm font-medium text-indigo-600" }
                                    else { "text-sm text-gray-500" }
                                }>
                                    {step.label}
                                </span>
                            </li>
                        }
                    }).collect_view()}
                </ol>
            </nav>
            // Active step content
            <div>{children()}</div>
        </div>
    }
}

#[derive(Clone)]
pub struct WizardStep {
    pub label: &'static str,
}
