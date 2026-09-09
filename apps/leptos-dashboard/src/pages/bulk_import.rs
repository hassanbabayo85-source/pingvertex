use crate::api::monitoring;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;

fn format_units(units: i64, currency: &str) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let amount = units.unsigned_abs();
    format!(
        "{sign}{currency} {}.{:06}",
        amount / 1_000_000,
        amount % 1_000_000
    )
}

fn error_csv(value: &monitoring::BulkImportPreview) -> String {
    let mut csv = String::from("row,error\n");
    for item in &value.errors {
        for message in &item.errors {
            let escaped = message.replace('"', "\"\"");
            csv.push_str(&format!("{},\"{}\"\n", item.row, escaped));
        }
    }
    format!(
        "data:text/csv;charset=utf-8,{}",
        js_sys::encode_uri_component(&csv)
            .as_string()
            .unwrap_or_default()
    )
}

#[component]
pub fn BulkImportPage() -> impl IntoView {
    let preview = RwSignal::new(None::<monitoring::BulkImportPreview>);
    let job = RwSignal::new(None::<monitoring::BulkImportJob>);
    let file_bytes = RwSignal::new(None::<Vec<u8>>);
    let file_name = RwSignal::new(String::new());
    let forecast_days = RwSignal::new("30".to_owned());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_file_change = move |event: web_sys::Event| {
        let input: HtmlInputElement = event_target(&event);
        let Some(file) = input.files().and_then(|files| files.item(0)) else {
            return;
        };
        file_name.set(file.name());
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match JsFuture::from(file.array_buffer()).await {
                Ok(buffer) => {
                    file_bytes.set(Some(js_sys::Uint8Array::new(&buffer).to_vec()));
                }
                Err(_) => error.set(Some("Could not read the selected CSV file.".to_owned())),
            }
            busy.set(false);
        });
    };

    let preview_import = move |_| {
        let Some(bytes) = file_bytes.get() else {
            error.set(Some("Choose a CSV file first.".to_owned()));
            return;
        };
        let name = file_name.get();
        let days = forecast_days
            .get()
            .parse::<u32>()
            .unwrap_or(30)
            .clamp(1, 3650);
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match monitoring::bulk_import_preview(bytes, name, days).await {
                Ok(value) => preview.set(Some(value)),
                Err(value) => error.set(Some(value)),
            }
            busy.set(false);
        });
    };

    let confirm_import = move |_| {
        let Some(bytes) = file_bytes.get() else {
            return;
        };
        let Some(value) = preview.get() else {
            return;
        };
        if value.valid_rows == 0 {
            error.set(Some("There are no valid rows to import.".to_owned()));
            return;
        }
        let name = file_name.get();
        let days = forecast_days
            .get()
            .parse::<u32>()
            .unwrap_or(30)
            .clamp(1, 3650);
        let key = format!("bulk-{}", js_sys::Date::now() as u64);
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match monitoring::bulk_import_confirm(bytes, name, days, key).await {
                Ok(value) => job.set(Some(value.job)),
                Err(value) => error.set(Some(value)),
            }
            busy.set(false);
        });
    };

    Effect::new(move |_| {
        let Some(current) = job.get() else {
            return;
        };
        let job_id = current.id;
        spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(1500).await;
                match monitoring::bulk_import_job(&job_id).await {
                    Ok(value) => {
                        let done =
                            matches!(value.status.as_str(), "COMPLETED" | "FAILED" | "CANCELLED");
                        job.set(Some(value));
                        if done {
                            break;
                        }
                    }
                    Err(value) => {
                        error.set(Some(value));
                        break;
                    }
                }
            }
        });
    });

    view! {
            <div class="feature-page">
                <header class="page-header">
                    <div>
                        <span class="eyebrow">"MONITORS"</span>
                        <h1>"Bulk import"</h1>
                        <p class="muted">"Validate the CSV, review the estimate, then create monitors in background batches."</p>
                    </div>
                </header>
                {move || error.get().map(|message| view! { <div class="error" role="alert">{message}</div> })}
                <section class="card">
                    <div class="form-grid">
                        <label>
                            "CSV file"
                            <input type="file" accept=".csv,text/csv" on:change=on_file_change />
                        </label>
                        <label>
                            "Forecast days"
                            <input
                                type="number"
                                min="1"
                                max="3650"
                                prop:value=move || forecast_days.get()
                                on:input=move |event| forecast_days.set(event_target_value(&event))
                            />
                        </label>
                    </div>
                    <p class="muted">"Maximum 10,000 data rows and 1 MB. Regions use semicolons (one supported region per monitor); authentication is explicitly limited to auth_type=none."</p>
                    <button class="button" disabled=move || busy.get() on:click=preview_import>
                        {move || if busy.get() { "Working…" } else { "Preview import" }}
                    </button>
                </section>
                {move || preview.get().map(|value| view! {
                    <section class="card">
                        <h2>"Preview"</h2>
                        <div class="metric-grid">
                            <div><strong>{value.total_rows}</strong><span>"Rows"</span></div>
                            <div><strong>{value.valid_rows}</strong><span>"Valid"</span></div>
                            <div><strong>{value.invalid_rows}</strong><span>"Invalid"</span></div>
                            <div><strong>{value.estimated_execution_quantity}</strong><span>"Estimated checks"</span></div>
                            <div><strong>{format_units(value.estimated_payg_cost_units, &value.currency)}</strong><span>"Estimated usage"</span></div>
    </div>
                        <div class="stack-form">
                            <p class="muted">"Pricing is server-resolved and pinned in the import job. Unit prices:"</p>
    </div>
                        {if value.errors.is_empty() {
                            view! { <p class="muted">"No validation errors."</p> }.into_any()
                        } else {
                            view! {
                                <details>
                                    <summary>{format!("{} validation errors", value.errors.len())}</summary>
                                    <ul>
                                        {value.errors.iter().take(100).map(|item| view! { <li>{format!("Row {}: {}", item.row, item.errors.join("; "))}</li> }).collect_view()}
                                    </ul>
                                </details>
                            }.into_any()
                        }}
                        {if value.errors.is_empty() {
                            ().into_any()
                        } else {
                            view! {
                                <a class="btn" download="bulk-import-errors.csv" href={
                                    let csv_value = value.clone();
                                    move || error_csv(&csv_value)
                                }>
                                    "Download errors"
                                </a>
                            }.into_any()
                        }}
                        <button class="button" disabled=move || busy.get() || value.valid_rows == 0 on:click=confirm_import>
                            "Confirm and import"
                        </button>
                    </section>
                })}
                {move || job.get().map(|value| view! {
                    <section class="card">
                        <h2>"Import progress"</h2>
                        <p>{format!("{} / {} processed", value.processed_rows, value.total_rows)}</p>
                        <progress max=value.total_rows value=value.processed_rows></progress>
                        <p class="muted">{format!("{} created · {} failed · {} invalid · {}", value.created_rows, value.failed_rows, value.invalid_rows, value.status)}</p>
                    </section>
                })}
            </div>
        }
}
