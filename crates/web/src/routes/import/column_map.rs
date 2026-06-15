use leptos::prelude::*;

/// Static reference table showing the expected CSV column layout.
#[component]
pub fn ColumnMapTable() -> impl IntoView {
    let columns = vec![
        ("set_id", "Set identifier, e.g. \"sv1\"", true),
        ("number", "Collector number, e.g. \"025\"", true),
        ("condition", "Condition: NM, LP, MP, HP, DMG (raw cards)", false),
        ("quantity", "Integer quantity (raw cards; defaults to 1)", false),
        ("grader", "Grading company: PSA, BGS, CGC, SGC (graded cards)", false),
        ("cert_number", "Grading cert number (graded cards)", false),
        ("grade", "Numeric grade, e.g. 9.5 (graded cards)", false),
        ("acquisition_cost_usd", "Purchase price in USD, e.g. 12.50", false),
        ("notes", "Free-text notes", false),
    ];

    view! {
        <div class="column-map-table">
            <h3>"Expected CSV Columns"</h3>
            <table>
                <thead>
                    <tr>
                        <th>"Column"</th>
                        <th>"Description"</th>
                        <th>"Required"</th>
                    </tr>
                </thead>
                <tbody>
                    {columns.into_iter().map(|(name, desc, required)| view! {
                        <tr>
                            <td><code>{name}</code></td>
                            <td>{desc}</td>
                            <td>{if required { "Yes" } else { "No" }}</td>
                        </tr>
                    }).collect_view()}
                </tbody>
            </table>
            <p class="note">
                "At least one of "
                <code>"condition"</code>
                " or "
                <code>"grader"</code>
                "/"
                <code>"cert_number"</code>
                " must be present. "
                "Rows with both are treated as graded cards."
            </p>
        </div>
    }
}
