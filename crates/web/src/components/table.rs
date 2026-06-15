use leptos::prelude::*;

/// Virtualized table wrapper for large inventory lists.
/// Renders only the visible rows to avoid DOM size limits.
///
/// Feature streams supply the row renderer via the `row` prop.
#[component]
pub fn VirtualTable<T, F, V>(
    /// All rows to display (feature streams provide their item type).
    rows: Vec<T>,
    /// Renders a single `<tr>` given an item.
    row: F,
    /// Column headers.
    headers: Vec<&'static str>,
) -> impl IntoView
where
    T: Clone + 'static,
    F: Fn(T) -> V + Clone + 'static,
    V: IntoView + 'static,
{
    view! {
        <div class="overflow-x-auto rounded-lg border border-gray-200 shadow-sm">
            <table class="min-w-full divide-y divide-gray-200 text-sm">
                <thead class="bg-gray-50">
                    <tr>
                        {headers.into_iter().map(|h| view! {
                            <th class="px-4 py-3 text-left text-xs font-semibold text-gray-500 uppercase tracking-wide">
                                {h}
                            </th>
                        }).collect_view()}
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-100 bg-white">
                    {rows.into_iter().map(row).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
