use crate::api::proxy_checks::{
    self, ProxyInput, ProxyJob, ProxyResult, ProxyResultBody, MAX_JSON_BYTES, MAX_PROXIES,
};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlAnchorElement, HtmlInputElement};

const POLL_INTERVAL_MS: u32 = 2_000;
const MAX_POLL_ATTEMPTS: u32 = 150;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResultFilter {
    All,
    Working,
    Failed,
    Http,
    Https,
    Socks5,
    HttpsCapable,
}

impl ResultFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Working => "Working",
            Self::Failed => "Failed",
            Self::Http => "HTTP",
            Self::Https => "HTTPS",
            Self::Socks5 => "SOCKS5",
            Self::HttpsCapable => "HTTPS-capable",
        }
    }
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "canceled")
}

fn human_error_code(code: &str) -> &'static str {
    match code {
        "PROXY_PRIVATE_IP_BLOCKED" | "PRIVATE_IP_BLOCKED" => {
            "This proxy points to a private or restricted address."
        }
        "JOB_FAILED" => "The proxy check could not be completed.",
        "DNS_FAILED" => "The proxy hostname could not be resolved.",
        "INVALID_PROXY" => "This proxy is invalid or uses a disallowed port.",
        "CONNECTION_REFUSED" => "The proxy refused the connection.",
        "CONNECTION_TIMEOUT" | "TARGET_TIMEOUT" => "The proxy connection timed out.",
        "AUTH_FAILED" => "The proxy rejected its credentials.",
        "TLS_FAILED" => "The proxy TLS connection could not be established.",
        "HTTP_PROXY_FAILED" | "HTTPS_PROXY_FAILED" | "SOCKS5_FAILED" => {
            "The proxy protocol check failed."
        }
        _ => "The proxy could not be reached.",
    }
}

fn humanize_api_error(message: String) -> String {
    for code in [
        "PROXY_PRIVATE_IP_BLOCKED",
        "PRIVATE_IP_BLOCKED",
        "JOB_FAILED",
        "DNS_FAILED",
        "INVALID_PROXY",
        "CONNECTION_REFUSED",
        "CONNECTION_TIMEOUT",
        "TARGET_TIMEOUT",
        "AUTH_FAILED",
        "TLS_FAILED",
        "HTTP_PROXY_FAILED",
        "HTTPS_PROXY_FAILED",
        "SOCKS5_FAILED",
    ] {
        if message.contains(code) {
            return human_error_code(code).to_string();
        }
    }
    message
}

fn result_error(result: &ProxyResultBody) -> String {
    match result.error_code.as_deref() {
        Some(code) => human_error_code(code).to_string(),
        None => result
            .error_message
            .clone()
            .unwrap_or_else(|| "The proxy could not be reached.".into()),
    }
}

fn matches_filter(row: &ProxyResult, filter: ResultFilter) -> bool {
    let body = row.result.as_ref();
    match filter {
        ResultFilter::All => true,
        ResultFilter::Working => body.is_some_and(|r| r.reachable),
        ResultFilter::Failed => body.is_some_and(|r| !r.reachable),
        ResultFilter::Http => row.scheme.eq_ignore_ascii_case("http"),
        ResultFilter::Https => row.scheme.eq_ignore_ascii_case("https"),
        ResultFilter::Socks5 => row.scheme.eq_ignore_ascii_case("socks5"),
        ResultFilter::HttpsCapable => body.is_some_and(|r| r.supports_https),
    }
}

fn csv_cell(value: &str) -> String {
    // RFC 4180 quoting plus spreadsheet-formula hardening. Hosts and errors
    // normally come from server-validated data, but exports must remain safe
    // if a future backend field ever contains a formula-like value.
    let value = if matches!(value.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
        format!("'{}", value)
    } else {
        value.to_string()
    };
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn result_to_csv(rows: &[ProxyResult]) -> String {
    let mut out = String::from(
        "host,port,scheme,reachable,latency_ms,supports_https,country_code,error_code\n",
    );
    for row in rows {
        let body = row.result.as_ref();
        let fields = [
            csv_cell(&row.host),
            row.port.to_string(),
            csv_cell(&row.scheme),
            body.is_some_and(|r| r.reachable).to_string(),
            body.and_then(|r| r.latency_ms)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            body.is_some_and(|r| r.supports_https).to_string(),
            body.and_then(|r| r.country_code.clone())
                .unwrap_or_default(),
            body.and_then(|r| r.error_code.clone()).unwrap_or_default(),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

fn download_csv(rows: &[ProxyResult], filename: &str) -> Result<(), String> {
    let csv = result_to_csv(rows);
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(&csv));
    let blob = web_sys::Blob::new_with_str_sequence(&parts)
        .map_err(|_| "Could not prepare the CSV export.".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "Could not create the CSV download.".to_string())?;

    let window = web_sys::window().ok_or("Browser window is unavailable.")?;
    let document = window
        .document()
        .ok_or("Browser document is unavailable.")?;
    let anchor = document
        .create_element("a")
        .map_err(|_| "Could not prepare the CSV download.".to_string())?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|_| "Could not prepare the CSV download.".to_string())?;

    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

async fn copy_text(value: String) -> Result<(), String> {
    let window = web_sys::window().ok_or("Browser window is unavailable.")?;
    let navigator = window.navigator();
    let clipboard = js_sys::Reflect::get(&navigator.into(), &JsValue::from_str("clipboard"))
        .map_err(|_| "Clipboard access is unavailable.".to_string())?;
    if clipboard.is_undefined() || clipboard.is_null() {
        return Err("Clipboard access is unavailable.".into());
    }

    let write_text = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))
        .map_err(|_| "Clipboard access is unavailable.".to_string())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "Clipboard access is unavailable.".to_string())?;
    let promise = write_text
        .call1(&clipboard, &JsValue::from_str(&value))
        .map_err(|_| "Could not copy the working proxies.".to_string())?;
    JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|_| "Could not copy the working proxies.".to_string())?;
    Ok(())
}

fn parse_input_for_preview(input: &str) -> (usize, Option<String>) {
    match proxy_checks::parse_proxy_text(input) {
        Ok(items) => (items.len(), None),
        Err(error) => {
            let count = input
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .count()
                .min(MAX_PROXIES);
            (count, Some(error))
        }
    }
}

fn safe_filename_suffix(filter: ResultFilter) -> &'static str {
    match filter {
        ResultFilter::All => "all",
        ResultFilter::Working => "working",
        ResultFilter::Failed => "failed",
        _ => "filtered",
    }
}

#[component]
pub fn ProxyChecksPage() -> impl IntoView {
    let input = RwSignal::new(String::new());
    let input_mode = RwSignal::new("paste".to_string());
    let job = RwSignal::new(None::<ProxyJob>);
    let results = RwSignal::new(Vec::<ProxyResult>::new());
    let active_filter = RwSignal::new(ResultFilter::All);
    let error = RwSignal::new(None::<String>);
    let input_error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let timed_out = RwSignal::new(false);
    let poll_generation = RwSignal::new(0u64);

    let submit_items = Callback::new(move |items: Vec<ProxyInput>| {
        error.set(None);
        input_error.set(None);
        if items.is_empty() {
            input_error.set(Some("Enter at least one proxy.".into()));
            return;
        }
        if items.len() > MAX_PROXIES {
            input_error.set(Some("Maximum 500 proxies per job.".into()));
            return;
        }

        busy.set(true);
        timed_out.set(false);
        results.set(Vec::new());
        poll_generation.update(|value| *value = value.wrapping_add(1));
        let generation = poll_generation.get_untracked();

        spawn_local(async move {
            match proxy_checks::create(&proxy_checks::CreateProxyCheckRequest { proxies: items })
                .await
            {
                Ok(created) => {
                    let id = created.id.clone();
                    job.set(Some(created));

                    // Required immediate GET after job creation.
                    match proxy_checks::get(&id).await {
                        Ok(current) => {
                            let terminal = is_terminal(&current.status);
                            job.set(Some(current.clone()));
                            if terminal {
                                match proxy_checks::all_results(&id).await {
                                    Ok(rows) => results.set(rows),
                                    Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
                                }
                            }
                        }
                        Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
                    }

                    let should_poll = job
                        .get_untracked()
                        .as_ref()
                        .is_some_and(|current| !is_terminal(&current.status));

                    if should_poll {
                        for _attempt in 0..MAX_POLL_ATTEMPTS {
                            TimeoutFuture::new(POLL_INTERVAL_MS).await;

                            if poll_generation.get_untracked() != generation {
                                return;
                            }

                            match proxy_checks::get(&id).await {
                                Ok(current) => {
                                    let terminal = is_terminal(&current.status);
                                    job.set(Some(current.clone()));

                                    if terminal {
                                        match proxy_checks::all_results(&id).await {
                                            Ok(rows) => results.set(rows),
                                            Err(e) => {
                                                error.set(Some(humanize_api_error(e.to_string())))
                                            }
                                        }
                                        return;
                                    }

                                    // While running, load the first page for fast
                                    // feedback; terminal state loads all pages.
                                    match proxy_checks::results(&id, 100, 0, None, None).await {
                                        Ok(page) => results.set(page.items),
                                        Err(e) => {
                                            error.set(Some(humanize_api_error(e.to_string())))
                                        }
                                    }
                                }
                                Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
                            }
                        }

                        if poll_generation.get_untracked() == generation {
                            timed_out.set(true);
                        }
                    }
                }
                Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
            }
            busy.set(false);
        });
    });

    let submit = {
        move |ev: leptos::ev::SubmitEvent| {
            ev.prevent_default();
            match proxy_checks::parse_proxy_text(&input.get()) {
                Ok(items) => submit_items.run(items),
                Err(message) => input_error.set(Some(message)),
            }
        }
    };

    let on_file_change = {
        move |ev: leptos::ev::Event| {
            let target = event_target::<HtmlInputElement>(&ev);
            let Some(file) = target.files().and_then(|files| files.get(0)) else {
                return;
            };

            let name = file.name().to_ascii_lowercase();
            let is_json = name.ends_with(".json");
            let is_csv = name.ends_with(".csv");
            if !is_json && !is_csv {
                input_error.set(Some("Choose a .csv or .json file.".into()));
                return;
            }

            busy.set(true);
            input_error.set(None);
            spawn_local(async move {
                let bytes = match JsFuture::from(file.array_buffer()).await {
                    Ok(value) => js_sys::Uint8Array::new(&value).to_vec(),
                    Err(_) => {
                        input_error.set(Some("Could not read the selected file.".into()));
                        busy.set(false);
                        return;
                    }
                };

                if is_json && bytes.len() > MAX_JSON_BYTES {
                    input_error.set(Some("JSON input exceeds the 1 MB limit.".into()));
                    busy.set(false);
                    return;
                }

                let text = String::from_utf8(bytes)
                    .map_err(|_| "The selected file is not valid UTF-8.".to_string());

                match text {
                    Ok(text) => {
                        input.set(text.clone());
                        input_mode.set(if is_json { "json".into() } else { "csv".into() });
                        let parsed = if is_json {
                            proxy_checks::parse_json_input(&text)
                        } else {
                            proxy_checks::parse_proxy_text(&text)
                        };
                        match parsed {
                            Ok(items) => {
                                // submit_items owns the busy lifecycle while the
                                // job is being created and polled.
                                submit_items.run(items)
                            }
                            Err(message) => {
                                input_error.set(Some(message));
                                busy.set(false);
                            }
                        }
                    }
                    Err(message) => {
                        input_error.set(Some(message));
                        busy.set(false);
                    }
                }
            });
        }
    };

    let refresh = move |_| {
        let Some(current) = job.get_untracked() else {
            return;
        };
        let id = current.id;
        spawn_local(async move {
            match proxy_checks::get(&id).await {
                Ok(current) => job.set(Some(current)),
                Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
            }
            match proxy_checks::all_results(&id).await {
                Ok(rows) => results.set(rows),
                Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
            }
        });
    };

    let cancel = move |_| {
        let Some(current) = job.get_untracked() else {
            return;
        };
        poll_generation.update(|value| *value = value.wrapping_add(1));
        let id = current.id;
        spawn_local(async move {
            match proxy_checks::cancel(&id).await {
                Ok(current) => {
                    job.set(Some(current));
                    match proxy_checks::all_results(&id).await {
                        Ok(rows) => results.set(rows),
                        Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
                    }
                }
                Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
            }
            busy.set(false);
        });
    };

    let export = move |filter: ResultFilter| {
        let rows = results.get_untracked();
        let selected = rows
            .iter()
            .filter(|row| matches_filter(row, filter))
            .cloned()
            .collect::<Vec<_>>();
        if selected.is_empty() {
            error.set(Some(
                "There are no results to export for this selection.".into(),
            ));
            return;
        }
        let filename = format!("proxy-check-{}.csv", safe_filename_suffix(filter));
        if let Err(message) = download_csv(&selected, &filename) {
            error.set(Some(message));
        }
    };

    let copy_working = move |_| {
        let rows = results.get_untracked();
        let working = rows
            .iter()
            .filter(|row| row.result.as_ref().is_some_and(|r| r.reachable))
            .map(|row| {
                // Credentials are deliberately absent from result DTOs, so
                // this copy operation can never expose stored credentials.
                format!("{}://{}:{}", row.scheme, row.host, row.port)
            })
            .collect::<Vec<_>>()
            .join("\n");

        if working.is_empty() {
            error.set(Some("There are no working proxies to copy.".into()));
            return;
        }

        spawn_local(async move {
            if let Err(message) = copy_text(working).await {
                error.set(Some(message));
            }
        });
    };

    view! {
        <div class="feature-page proxy-check-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"TOOLS"</span>
                    <h1>"Proxy Checker"</h1>
                    <p class="muted">"Check up to 500 public proxies with server-side SSRF protection."</p>
                </div>
            </header>

            {move || error.get().map(|message| view! {
                <div class="error" role="alert">{message}</div>
            })}

            <section class="card proxy-input-card">
                <div class="proxy-input-tabs" role="tablist" aria-label="Proxy input format">
                    <button type="button" class:active=move || input_mode.get() == "paste">"Paste"</button>
                    <label class="btn proxy-file-button">
                        "CSV Upload"
                        <input type="file" accept=".csv,text/csv" on:change=on_file_change />
                    </label>
                    <label class="btn proxy-file-button">
                        "JSON"
                        <input type="file" accept=".json,application/json" on:change=on_file_change />
                    </label>
                </div>

                <form on:submit=submit class="stack-form">
                    <label for="proxy-list">"Paste proxies"</label>
                    <textarea
                        id="proxy-list"
                        rows="10"
                        spellcheck="false"
                        autocomplete="off"
                        prop:value=input
                        placeholder="http://1.2.3.4:8080&#10;socks5://2.3.4.5:1080"
                        on:input=move |e| {
                            input.set(event_target_value(&e));
                            input_mode.set("paste".into());
                            input_error.set(None);
                        }
                    ></textarea>

                    {move || {
                        let (count, preview_error) = parse_input_for_preview(&input.get());
                        view! {
                            <div class="proxy-input-meta">
                                <span>{format!("{} proxies detected", count)}</span>
                                {preview_error.map(|message| view! {
                                    <span class="proxy-inline-error">{message}</span>
                                })}
                            </div>
                        }
                    }}

                    <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                        {move || if busy.get() { "Submitting…" } else { "Check Proxies" }}
                    </button>
                </form>

                {move || input_error.get().map(|message| view! {
                    <div class="form-error" role="alert">{message}</div>
                })}
            </section>

            {move || job.get().map(|current| {
                let status = current.status.clone();
                let progress = if current.total_count == 0 {
                    0
                } else {
                    ((current.checked_count as f64 / current.total_count as f64) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u32
                };
                let terminal = is_terminal(&status);
                view! {
                    <section class="card proxy-progress-card">
                        <div class="section-heading">
                            <div>
                                <h2>{if terminal { "Check complete" } else { "Checking…" }}</h2>
                                <span class="muted">{status.clone()}</span>
                            </div>
                            <div class="proxy-actions">
                                <button class="btn" type="button" on:click=refresh>"Refresh"</button>
                                {if !terminal {
                                    view! {
                                        <button class="btn btn-danger" type="button" on:click=cancel disabled=move || busy.get()>"Cancel"</button>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}
                            </div>
                        </div>

                        <div class="proxy-progress-track" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=progress.to_string()>
                            <div class="proxy-progress-fill" style=move || format!("width: {}%;", progress)></div>
                        </div>
                        <div class="proxy-progress-stats">
                            <span><strong>{current.total_count}</strong>" Total"</span>
                            <span><strong>{current.checked_count}</strong>" Checked"</span>
                            <span><strong>{current.successful_count}</strong>" Working"</span>
                            <span><strong>{current.failed_count}</strong>" Failed"</span>
                            <span><strong>{format!("{}%", progress)}</strong>" Progress"</span>
                        </div>
                        {if status == "failed" {
                            current.failure_code.clone().map(|code| {
                                let message = match code.as_str() {
                                    "PAYG_BALANCE_EXHAUSTED" => "PAYG balance exhausted. Top up the wallet to resume proxy checks.",
                                    _ => "The proxy check job failed. Refresh to see whether it can be retried.",
                                };
                                view! { <div class="error" role="alert">{message}</div> }
                            }).into_any()
                        } else {
                            ().into_any()
                        }}
                    </section>
                }
            })}

            {move || timed_out.get().then(|| view! {
                <div class="error" role="status">
                    "The UI stopped polling after 5 minutes. The job may still be running; use Refresh when you return."
                </div>
            })}

            {move || if job.get().is_some() {
                let rows = results.get();
                let total = job.get().map(|j| j.total_count as usize).unwrap_or(rows.len());
                let working = rows.iter().filter(|r| r.result.as_ref().is_some_and(|b| b.reachable)).count();
                let failed = rows.iter().filter(|r| r.result.as_ref().is_some_and(|b| !b.reachable)).count();
                let https = rows
                    .iter()
                    .filter(|r| r.result.as_ref().is_some_and(|b| b.supports_https))
                    .count();
                let latencies = rows.iter().filter_map(|r| r.result.as_ref().and_then(|b| b.latency_ms));
                let latency_values = latencies.collect::<Vec<_>>();
                let avg_latency = if latency_values.is_empty() {
                    "—".into()
                } else {
                    format!("{} ms", latency_values.iter().copied().map(u64::from).sum::<u64>() / latency_values.len() as u64)
                };

                view! {
                    <section class="proxy-summary">
                        <div class="proxy-summary-card"><span>"TOTAL"</span><strong>{total}</strong></div>
                        <div class="proxy-summary-card"><span>"WORKING"</span><strong class="up">{working}</strong></div>
                        <div class="proxy-summary-card"><span>"FAILED"</span><strong class="down">{failed}</strong></div>
                        <div class="proxy-summary-card"><span>"HTTPS"</span><strong>{https}</strong></div>
                        <div class="proxy-summary-card"><span>"AVG LATENCY"</span><strong>{avg_latency}</strong></div>
                    </section>

                    <section class="card proxy-results-card">
                        <div class="section-heading">
                            <div>
                                <h2>"Results"</h2>
                                <span class="muted">{format!("{} loaded", rows.len())}</span>
                            </div>
                            <div class="proxy-export-actions">
                                <button class="btn" type="button" on:click=move |_| export(ResultFilter::All)>"Export All"</button>
                                <button class="btn" type="button" on:click=move |_| export(ResultFilter::Working)>"Export Working"</button>
                                <button class="btn" type="button" on:click=move |_| export(ResultFilter::Failed)>"Export Failed"</button>
                                <button class="btn" type="button" on:click=copy_working>"Copy Working"</button>
                            </div>
                        </div>

                        <div class="proxy-filters" role="toolbar" aria-label="Result filters">
                            {[
                                ResultFilter::All,
                                ResultFilter::Working,
                                ResultFilter::Failed,
                                ResultFilter::Http,
                                ResultFilter::Https,
                                ResultFilter::Socks5,
                                ResultFilter::HttpsCapable,
                            ].into_iter().map(|filter| {
                                let label = filter.label();
                                view! {
                                    <button
                                        type="button"
                                        class:active=move || active_filter.get() == filter
                                        on:click=move |_| active_filter.set(filter)
                                    >{label}</button>
                                }
                            }).collect_view()}
                        </div>

                        <div class="table-responsive">
                            <table class="proxy-results-table">
                                <thead>
                                    <tr>
                                        <th>"Host"</th>
                                        <th>"Port"</th>
                                        <th>"Scheme"</th>
                                        <th>"Status"</th>
                                        <th>"Latency"</th>
                                        <th>"HTTPS"</th>
                                        <th>"Country"</th>
                                        <th>"Error"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || results.get().into_iter()
                                        .filter(|row| matches_filter(row, active_filter.get()))
                                        .map(|row| {
                                            let body = row.result.clone();
                                            let status = body.as_ref().map(|r| if r.reachable { "Working" } else { "Failed" }).unwrap_or("Pending");
                                            let status_class = if status == "Working" { "up" } else if status == "Failed" { "down" } else { "" };
                                            let latency = body.as_ref().and_then(|r| r.latency_ms).map(|v| format!("{} ms", v)).unwrap_or_else(|| "—".into());
                                            let https = body.as_ref().is_some_and(|r| r.supports_https);
                                            let country = body.as_ref().and_then(|r| r.country_code.clone()).unwrap_or_else(|| "—".into());
                                            let error_text = body.as_ref().filter(|r| !r.reachable).map(result_error).unwrap_or_else(|| "—".into());
                                            let error_title = error_text.clone();
                                            view! {
                                                <tr>
                                                    <td>{row.host}</td>
                                                    <td>{row.port}</td>
                                                    <td>{row.scheme}</td>
                                                    <td><span class=status_class>{status}</span></td>
                                                    <td>{latency}</td>
                                                    <td>{if https { "Yes" } else { "No" }}</td>
                                                    <td>{country}</td>
                                                    <td title=error_title>{error_text}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    </section>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(reachable: bool, scheme: &str, https: bool) -> ProxyResult {
        ProxyResult {
            id: "id".into(),
            host: "8.8.8.8".into(),
            port: 8080,
            scheme: scheme.into(),
            result: Some(ProxyResultBody {
                reachable,
                latency_ms: Some(20),
                supports_https: https,
                country_code: Some("US".into()),
                anonymity: None,
                error_code: None,
                error_message: None,
            }),
        }
    }

    #[test]
    fn filters_working_failed_scheme_and_https_capable() {
        let working_http = row(true, "http", true);
        let failed_socks = row(false, "socks5", false);
        assert!(matches_filter(&working_http, ResultFilter::Working));
        assert!(!matches_filter(&working_http, ResultFilter::Failed));
        assert!(matches_filter(&working_http, ResultFilter::Http));
        assert!(matches_filter(&working_http, ResultFilter::HttpsCapable));
        assert!(matches_filter(&failed_socks, ResultFilter::Failed));
        assert!(matches_filter(&failed_socks, ResultFilter::Socks5));
    }

    #[test]
    fn csv_export_never_contains_credentials_fields() {
        let csv = result_to_csv(&[row(true, "http", true)]);
        assert!(csv.contains("host,port,scheme,reachable"));
        assert!(!csv.contains("username"));
        assert!(!csv.contains("password"));
        assert!(!csv.contains("credentials"));
    }

    #[test]
    fn error_codes_are_humanized() {
        assert_eq!(
            human_error_code("PROXY_PRIVATE_IP_BLOCKED"),
            "This proxy points to a private or restricted address."
        );
        assert_eq!(
            human_error_code("JOB_FAILED"),
            "The proxy check could not be completed."
        );
    }
}
