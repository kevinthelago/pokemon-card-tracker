//! Column mapping step — lets the user assign CSV headers to known fields.

use std::collections::HashMap;

use leptos::prelude::*;

use crate::routes::import::api::ColumnMap;

/// Logical field names the user must map to CSV headers.
const REQUIRED_FIELDS: &[(&str, &str, bool)] = &[
    ("set_code", "Set code (e.g. base1)", false),
    ("collector_number", "Collector number (e.g. 4)", false),
    ("printing_id", "TCG API printing ID (overrides set/number)", false),
    ("condition", "Condition / grade", true),
    ("quantity", "Quantity", true),
    ("acquisition_cost", "Acquisition cost (USD)", false),
    ("grader", "Grader (PSA / CGC / BGS)", false),
    ("cert_number", "Cert number", false),
    ("notes", "Notes", false),
];

#[component]
pub fn ColumnMappingStep(
    headers: Vec<String>,
    initial_map: ColumnMap,
    on_confirm: Callback<ColumnMap>,
    on_back: Callback<()>,
) -> impl IntoView {
    let map = RwSignal::new(initial_map);
    let error = RwSignal::<Option<String>>::new(None);

    let headers_with_empty: Vec<String> = {
        let mut v = vec![String::new()];
        v.extend(headers.clone());
        v
    };

    let confirm = move |_| {
        let current = map.get();
        // Validate: need (printing_id) or (set_code + collector_number)
        let identity_ok = current.printing_id.as_ref().map_or(false, |s| !s.is_empty())
            || (current.set_code.as_ref().map_or(false, |s| !s.is_empty())
                && current.collector_number.as_ref().map_or(false, |s| !s.is_empty()));
        let condition_ok = current.condition.as_ref().map_or(false, |s| !s.is_empty());
        let qty_ok = current.quantity.as_ref().map_or(false, |s| !s.is_empty());

        if !identity_ok {
            error.set(Some(
                "Provide either 'printing_id' or both 'set_code' and 'collector_number'".to_owned(),
            ));
            return;
        }
        if !condition_ok {
            error.set(Some("'condition' column mapping is required".to_owned()));
            return;
        }
        if !qty_ok {
            error.set(Some("'quantity' column mapping is required".to_owned()));
            return;
        }
        error.set(None);
        on_confirm.run(current);
    };

    let make_setter = |field: &'static str| {
        move |ev: leptos::ev::Event| {
            let val = event_target_value(&ev);
            let val = if val.is_empty() { None } else { Some(val) };
            map.update(|m| match field {
                "set_code" => m.set_code = val,
                "collector_number" => m.collector_number = val,
                "printing_id" => m.printing_id = val,
                "name" => m.name = val,
                "condition" => m.condition = val,
                "quantity" => m.quantity = val,
                "acquisition_cost" => m.acquisition_cost = val,
                "grader" => m.grader = val,
                "cert_number" => m.cert_number = val,
                "notes" => m.notes = val,
                _ => {}
            });
        }
    };

    let get_field = move |field: &str| -> String {
        let m = map.get();
        match field {
            "set_code" => m.set_code,
            "collector_number" => m.collector_number,
            "printing_id" => m.printing_id,
            "name" => m.name,
            "condition" => m.condition,
            "quantity" => m.quantity,
            "acquisition_cost" => m.acquisition_cost,
            "grader" => m.grader,
            "cert_number" => m.cert_number,
            "notes" => m.notes,
            _ => None,
        }
        .unwrap_or_default()
    };

    view! {
        <div class="space-y-6">
            <h2 class="text-lg font-semibold">"Map your columns"</h2>
            <p class="text-sm text-gray-600">
                "For each field below, choose which column in your CSV file contains that data."
            </p>

            <div class="overflow-x-auto">
                <table class="w-full text-sm border-collapse">
                    <thead>
                        <tr class="bg-gray-50 border-b">
                            <th class="text-left py-2 px-3 font-medium">"Field"</th>
                            <th class="text-left py-2 px-3 font-medium">"CSV column"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {REQUIRED_FIELDS.iter().map(|(field, label, required)| {
                            let field = *field;
                            let label = *label;
                            let required = *required;
                            let headers_ref = headers_with_empty.clone();
                            let current = get_field(field);
                            view! {
                                <tr class="border-b last:border-0">
                                    <td class="py-2 px-3">
                                        {label}
                                        {if required {
                                            view! { <span class="text-red-500 ml-1">"*"</span> }.into_any()
                                        } else {
                                            view! { <span /> }.into_any()
                                        }}
                                    </td>
                                    <td class="py-2 px-3">
                                        <select
                                            class="border rounded px-2 py-1 w-full text-sm"
                                            on:change=make_setter(field)
                                        >
                                            {headers_ref.iter().map(|h| {
                                                let h = h.clone();
                                                let selected = h == current;
                                                view! {
                                                    <option value=h.clone() selected=selected>{h.clone()}</option>
                                                }
                                            }).collect_view()}
                                        </select>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>

            {move || error.get().map(|e| view! {
                <p class="text-sm text-red-600 bg-red-50 border border-red-200 rounded p-3">{e}</p>
            })}

            <div class="flex gap-3">
                <button
                    type="button"
                    class="px-4 py-2 text-sm border rounded hover:bg-gray-50"
                    on:click=move |_| on_back.run(())
                >
                    "Back"
                </button>
                <button
                    type="button"
                    class="px-4 py-2 text-sm bg-blue-600 text-white rounded hover:bg-blue-700"
                    on:click=confirm
                >
                    "Continue"
                </button>
            </div>
        </div>
    }
}

/// Read-only preview table of the first N rows of the uploaded CSV.
#[component]
pub fn PreviewTable(
    headers: Vec<String>,
    rows: Vec<HashMap<String, String>>,
) -> impl IntoView {
    if rows.is_empty() {
        return view! {
            <p class="text-sm text-gray-500 italic">"No data rows to preview."</p>
        }
        .into_any();
    }

    view! {
        <div class="overflow-x-auto rounded border">
            <table class="text-xs border-collapse w-full">
                <thead>
                    <tr class="bg-gray-100">
                        {headers.iter().map(|h| view! {
                            <th class="px-3 py-2 text-left font-medium whitespace-nowrap border-b">{h.clone()}</th>
                        }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {rows.iter().map(|row| {
                        let headers_clone = headers.clone();
                        let row_clone = row.clone();
                        view! {
                            <tr class="border-b last:border-0 hover:bg-gray-50">
                                {headers_clone.iter().map(|h| {
                                    let val = row_clone.get(h).cloned().unwrap_or_default();
                                    view! {
                                        <td class="px-3 py-1.5 whitespace-nowrap">{val}</td>
                                    }
                                }).collect_view()}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}
