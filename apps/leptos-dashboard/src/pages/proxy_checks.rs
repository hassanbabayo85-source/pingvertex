use std::collections::HashSet;

use crate::api::proxy_checks::{
    self, ProxyInput, ProxyJob, ProxyPreview, ProxyResult, ProxyResultBody, MAX_JSON_BYTES,
    MAX_PROXIES,
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
    /// Proxy is alive (Working OR WorkingTargetUnreachable). The correct
    /// filter for "which proxies can I actually use?".
    Working,
    /// Proxy is alive but the target through it was unreachable. Only
    /// meaningful for the PART 3 outcome model.
    TargetFailed,
    /// Proxy itself failed. The only filter that means "dead proxy".
    ProxyFailed,
    /// Legacy alias retained for the old "Failed" label — matches any
    /// non-alive proxy OR any non-fully-working one, so that existing
    /// tests and users keep seeing what they saw before PART 3.
    Failed,
    Http,
    Https,
    Socks4,
    Socks5,
    HttpsCapable,
}

impl ResultFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Working => "Working",
            Self::TargetFailed => "Target failed",
            Self::ProxyFailed => "Proxy failed",
            Self::Failed => "Any failed",
            Self::Http => "HTTP",
            Self::Https => "HTTPS",
            Self::Socks4 => "SOCKS4",
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
        "CONNECTION_TIMEOUT" => "The proxy connection timed out.",
        "AUTH_FAILED" => "The proxy rejected its credentials.",
        "TLS_FAILED" => "The proxy TLS connection could not be established.",
        "HTTP_PROXY_FAILED" | "HTTPS_PROXY_FAILED" | "SOCKS4_FAILED" | "SOCKS5_FAILED" => {
            "The proxy protocol check failed."
        }
        // PART 3 — the target through the proxy failed, but the proxy
        // itself is alive. These must NOT be shown as "the proxy could
        // not be reached" — the proxy was reached successfully.
        "TARGET_TIMEOUT" | "TARGET_CONNECT_TIMEOUT" => {
            "The target did not respond in time through this proxy."
        }
        "TARGET_TLS_FAILED" => "TLS to the target through this proxy failed.",
        "TARGET_HTTP_ERROR" | "TARGET_CONNECT_HTTP_ERROR" => {
            "The target returned an unexpected response through this proxy."
        }
        "TARGET_CONNECT_FAILED" => "The proxy could not reach the target.",
        "TARGET_CONNECT_AUTH_FAILED" => {
            "The proxy rejected the CONNECT tunnel for this target."
        }
        _ => "The proxy could not be reached.",
    }
}

fn humanize_api_error(message: String) -> String {
    for code in [
        "PROXY_PRIVATE_IP_BLOCKED", "PRIVATE_IP_BLOCKED", "JOB_FAILED", "DNS_FAILED",
        "INVALID_PROXY", "CONNECTION_REFUSED", "CONNECTION_TIMEOUT", "TARGET_TIMEOUT",
        "AUTH_FAILED", "TLS_FAILED", "HTTP_PROXY_FAILED", "HTTPS_PROXY_FAILED",
        "SOCKS4_FAILED", "SOCKS5_FAILED",
        "TARGET_TIMEOUT", "TARGET_TLS_FAILED", "TARGET_HTTP_ERROR",
        "TARGET_CONNECT_FAILED", "TARGET_CONNECT_TIMEOUT",
        "TARGET_CONNECT_HTTP_ERROR", "TARGET_CONNECT_AUTH_FAILED",
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
        // PART 3 — "Working" means the PROXY is alive. A target-side
        // failure must not hide a good proxy from the user.
        ResultFilter::Working => body.is_some_and(|r| r.is_alive()),
        ResultFilter::TargetFailed => body.is_some_and(|r| r.is_target_failure()),
        ResultFilter::ProxyFailed => body.is_some_and(|r| {
            r.effective_outcome() == "proxy_failed"
        }),
        // Legacy "Failed" filter — kept as a superset of the previous
        // `!reachable` behaviour so existing tests/users are unchanged.
        ResultFilter::Failed => body.is_some_and(|r| !r.reachable),
        ResultFilter::Http => row.scheme.eq_ignore_ascii_case("http"),
        ResultFilter::Https => row.scheme.eq_ignore_ascii_case("https"),
        ResultFilter::Socks4 => row.scheme.eq_ignore_ascii_case("socks4"),
        ResultFilter::Socks5 => row.scheme.eq_ignore_ascii_case("socks5"),
        ResultFilter::HttpsCapable => body.is_some_and(|r| r.supports_https),
    }
}

fn csv_cell(value: &str) -> String {
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
        "host,port,scheme,outcome,reachable,total_ms,connect_ms,tls_ms,ttfb_ms,http_status,supports_https,country_code,error_code\n",
    );
    for row in rows {
        let body = row.result.as_ref();
        let fields = [
            csv_cell(&row.host),
            row.port.to_string(),
            csv_cell(&row.scheme),
            csv_cell(body.map(|r| r.effective_outcome()).unwrap_or("pending")),
            body.is_some_and(|r| r.reachable).to_string(),
            body.and_then(|r| r.effective_total_time_ms()).map(|v| v.to_string()).unwrap_or_default(),
            body.and_then(|r| r.connection_latency_ms).map(|v| v.to_string()).unwrap_or_default(),
            body.and_then(|r| r.tls_handshake_latency_ms).map(|v| v.to_string()).unwrap_or_default(),
            body.and_then(|r| r.ttfb_ms).map(|v| v.to_string()).unwrap_or_default(),
            body.and_then(|r| r.http_status_code).map(|v| v.to_string()).unwrap_or_default(),
            body.is_some_and(|r| r.supports_https).to_string(),
            body.and_then(|r| r.country_code.clone()).unwrap_or_default(),
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
    let document = window.document().ok_or("Browser document is unavailable.")?;
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
    let uploaded_filename = RwSignal::new(None::<String>);
    let uploaded_proxy_count = RwSignal::new(0usize);
    let job = RwSignal::new(None::<ProxyJob>);
    let results = RwSignal::new(Vec::<ProxyResult>::new());
    let active_filter = RwSignal::new(ResultFilter::All);
    let error = RwSignal::new(None::<String>);
    let input_error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let timed_out = RwSignal::new(false);
    let poll_generation = RwSignal::new(0u64);
    let preview = RwSignal::new(None::<ProxyPreview>);
    let preview_busy = RwSignal::new(false);

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

        // If a fresh preview is available, restrict the submission to
        // only the entries it accepted. This is how duplicate suppression
        // actually happens on the wire: the preview endpoint classifies
        // every unit, and here we map its accepted positions back to the
        // original parsed input (which still carries credentials the
        // summary deliberately omitted). Without a fresh preview, we
        // submit everything as before — no behavior regression.
        let items = match preview.get_untracked() {
            Some(p) => {
                let accepted: HashSet<usize> =
                    p.accepted.iter().map(|e| e.position).collect();
                items
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| accepted.contains(&(i + 1)))
                    .map(|(_, item)| item)
                    .collect::<Vec<ProxyInput>>()
            }
            None => items,
        };

        if items.is_empty() {
            input_error.set(Some(
                "None of the entered proxies are valid and unique. Fix the input and try again."
                    .into(),
            ));
            return;
        }

        busy.set(true);
        timed_out.set(false);
        results.set(Vec::new());
        poll_generation.update(|value| *value = value.wrapping_add(1));
        let generation = poll_generation.get_untracked();

        spawn_local(async move {
            match proxy_checks::create(&proxy_checks::CreateProxyCheckRequest { proxies: items }).await {
                Ok(created) => {
                    let id = created.id.clone();
                    job.set(Some(created));

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

                    let should_poll = job.get_untracked().as_ref().is_some_and(|current| !is_terminal(&current.status));

                    if should_poll {
                        for _attempt in 0..MAX_POLL_ATTEMPTS {
                            TimeoutFuture::new(POLL_INTERVAL_MS).await;
                            if poll_generation.get_untracked() != generation { return; }

                            match proxy_checks::get(&id).await {
                                Ok(current) => {
                                    let terminal = is_terminal(&current.status);
                                    job.set(Some(current.clone()));

                                    if terminal {
                                        match proxy_checks::all_results(&id).await {
                                            Ok(rows) => results.set(rows),
                                            Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
                                        }
                                        return;
                                    }

                                    match proxy_checks::results(&id, 100, 0, None, None).await {
                                        Ok(page) => results.set(page.items),
                                        Err(e) => error.set(Some(humanize_api_error(e.to_string()))),
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
            let Some(file) = target.files().and_then(|files| files.get(0)) else { return; };
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

                let text = String::from_utf8(bytes).map_err(|_| "The selected file is not valid UTF-8.".to_string());

                match text {
                    Ok(text) => {
                        input.set(text.clone());
                        input_mode.set(if is_json { "json".into() } else { "csv".into() });
                        let parsed = if is_json { proxy_checks::parse_json_input(&text) } else { proxy_checks::parse_proxy_text(&text) };
                        match parsed {
                            Ok(items) => {
                                uploaded_filename.set(Some(file.name()));
                                uploaded_proxy_count.set(items.len());
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
        let Some(current) = job.get_untracked() else { return; };
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
        let Some(current) = job.get_untracked() else { return; };
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
        let selected = rows.iter().filter(|row| matches_filter(row, filter)).cloned().collect::<Vec<_>>();
        if selected.is_empty() {
            error.set(Some("There are no results to export for this selection.".into()));
            return;
        }
        let filename = format!("proxy-check-{}.csv", safe_filename_suffix(filter));
        if let Err(message) = download_csv(&selected, &filename) {
            error.set(Some(message));
        }
    };

    let copy_working = move |_| {
        let rows = results.get_untracked();
        let working = rows.iter()
            .filter(|row| row.result.as_ref().is_some_and(|r| r.reachable))
            .map(|row| format!("{}://{}:{}", row.scheme, row.host, row.port))
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
            // Hero Header
            <header class="page-header proxy-hero-header">
                <div class="proxy-hero-left">
                    <div class="proxy-hero-icon">"🛡️"</div>
                    <div class="proxy-hero-text">
                        <span class="eyebrow">"TOOLS"</span>
                        <h1>"Proxy Checker"</h1>
                        <p>"Check up to 500 public proxies with server-side SSRF protection.<br/>Fast. Secure. Reliable."</p>
                    </div>
                </div>
                <div class="proxy-hero-badges">
                    <div class="proxy-feature-badge badge-indigo">
                        <span class="badge-icon">"🛡️"</span>
                        <div class="badge-text">
                            <strong>"SSRF Protection"</strong>
                            <span>"Prevents server-side attacks"</span>
                        </div>
                    </div>
                    <div class="proxy-feature-badge badge-green">
                        <span class="badge-icon">"⚡"</span>
                        <div class="badge-text">
                            <strong>"High Performance"</strong>
                            <span>"Up to 500 proxies per job"</span>
                        </div>
                    </div>
                    <div class="proxy-feature-badge badge-green">
                        <span class="badge-icon">"📊"</span>
                        <div class="badge-text">
                            <strong>"Real-time Results"</strong>
                            <span>"Detailed status & logs"</span>
                        </div>
                    </div>
                </div>
            </header>

            {move || error.get().map(|message| view! {
                <div class="error" role="alert">{message}</div>
            })}

            // Main Layout: Input + Quick Info Sidebar
            <div class="proxy-layout">
                <div class="proxy-main-column">
                    <section class="card proxy-input-card proxy-input-card-premium">
                        <div class="proxy-input-hero">
                            <div>
                                <span class="proxy-section-kicker">"PROXY SOURCE"</span>
                                <h2>"Add proxies to inspect"</h2>
                                <p class="muted">"Upload a CSV/JSON list or paste proxy endpoints. Up to 500 proxies per job."</p>
                            </div>
                            <div class="proxy-source-orb" aria-hidden="true"><span>"↗"</span></div>
                        </div>

                        <div class="proxy-input-tabs" role="tablist" aria-label="Proxy input format">
                            <button type="button" class:active=move || input_mode.get() == "paste" on:click=move |_| input_mode.set("paste".into())>
                                <span class="proxy-tab-icon">"⌨"</span> " Paste"
                            </button>
                            <label class="btn proxy-file-button">
                                <span class="proxy-tab-icon">"CSV"</span> " Upload CSV"
                                <input type="file" accept=".csv,text/csv" on:change=on_file_change />
                            </label>
                            <label class="btn proxy-file-button">
                                <span class="proxy-tab-icon">"JSON"</span> " Upload JSON"
                                <input type="file" accept=".json,application/json" on:change=on_file_change />
                            </label>
                        </div>

                        {move || uploaded_filename.get().map(|filename| view! {
                            <div class="proxy-uploaded-file">
                                <div class="proxy-file-icon">"▤"</div>
                                <div class="proxy-file-details">
                                    <strong>{filename.clone()}</strong>
                                    <span>{format!("{} file · {} proxies detected", if filename.to_ascii_lowercase().ends_with(".json") { "JSON" } else { "CSV" }, uploaded_proxy_count.get())}</span>
                                </div>
                                <div class="proxy-file-state"><span class="proxy-file-dot"></span> " Ready"</div>
                            </div>
                        })}

                        <form on:submit=submit class="stack-form">
                            <label for="proxy-list">"Paste proxies"</label>
                            <div class="proxy-editor-shell">
                                <div class="proxy-editor-bar">
                                    <span></span><span></span><span></span>
                                    <small>"PROXY LIST"</small>
                                </div>
                                <textarea id="proxy-list" rows="10" spellcheck="false" autocomplete="off"
                                    prop:value=input
                                    placeholder="http://1.2.3.4:8080&#10;socks4://2.3.4.5:1080&#10;socks5://3.4.5.6:1080"
                                    on:input=move |e| {
                                        input.set(event_target_value(&e));
                                        input_mode.set("paste".into());
                                        uploaded_filename.set(None);
                                        uploaded_proxy_count.set(0);
                                        input_error.set(None);
                                        // Any edit invalidates the previous preview.
                                        preview.set(None);
                                    }
                                ></textarea>
                            </div>

                            {move || {
                                let (count, preview_error) = parse_input_for_preview(&input.get());
                                view! {
                                    <div class="proxy-input-meta proxy-input-meta-premium">
                                        <span><strong>{count}</strong> " proxies detected"</span>
                                        <span class="proxy-input-limit">"MAX 500"</span>
                                        {preview_error.map(|message| view! { <span class="proxy-inline-error">{message}</span> })}
                                    </div>
                                }
                            }}

                            <div class="proxy-submit-row">
                                <button
                                    class="btn"
                                    type="button"
                                    disabled=move || preview_busy.get() || input.get().trim().is_empty()
                                    on:click=move |_| {
                                        error.set(None);
                                        input_error.set(None);
                                        let content = input.get_untracked();
                                        if content.trim().is_empty() {
                                            input_error.set(Some("Enter at least one proxy.".into()));
                                            return;
                                        }
                                        let format = if input_mode.get_untracked() == "json" { "json" } else { "csv" };
                                        preview_busy.set(true);
                                        spawn_local(async move {
                                            match proxy_checks::preview(format, &content).await {
                                                Ok(p) => {
                                                    preview.set(Some(p));
                                                }
                                                Err(e) => {
                                                    preview.set(None);
                                                    input_error.set(Some(format!("Preview failed: {}", e)));
                                                }
                                            }
                                            preview_busy.set(false);
                                        });
                                    }
                                >
                                    {move || if preview_busy.get() { "Previewing…" } else { "Preview" }}
                                </button>
                                <button class="btn btn-primary proxy-check-button" type="submit" disabled=move || busy.get()>
                                    <span>"◉"</span>
                                    {move || if busy.get() { "Checking…" } else { "Check Proxies" }}
                                </button>
                            </div>
                        </form>

                        {move || preview.get().map(|p| {
                            let rejected_len = p.rejected.len();
                            let duplicates_len = p.duplicates.len();
                            view! {
                                <section class="card proxy-preview-card">
                                    <div class="section-heading">
                                        <h3>"Import summary"</h3>
                                        <span class="muted">"Backend validation, before submission"</span>
                                    </div>
                                    <div class="proxy-preview-counts">
                                        <div><span>"TOTAL"</span><strong>{p.total}</strong></div>
                                        <div><span>"ACCEPTED"</span><strong class="up">{p.accepted.len()}</strong></div>
                                        <div><span>"DUPLICATES"</span><strong>{p.duplicate_count}</strong></div>
                                        <div><span>"INVALID"</span><strong class="down">{p.invalid}</strong></div>
                                        <div><span>"REJECTED"</span><strong class="down">{p.rejected_count}</strong></div>
                                    </div>
                                    {if rejected_len > 0 {
                                        let items = p.rejected.iter().take(50).map(|r| {
                                            view! {
                                                <li>
                                                    <code>{format!("line {}", r.position)}</code>
                                                    " — " {r.reason.clone()}
                                                </li>
                                            }
                                        }).collect_view();
                                        view! {
                                            <details class="proxy-preview-details">
                                                <summary>{format!("{} rejected", rejected_len)}</summary>
                                                <ul>{items}</ul>
                                            </details>
                                        }.into_any()
                                    } else { ().into_any() }}
                                    {if duplicates_len > 0 {
                                        let items = p.duplicates.iter().take(50).map(|d| {
                                            view! {
                                                <li>
                                                    <code>{format!("line {}", d.position)}</code>
                                                    " — duplicate of line "
                                                    <code>{d.first_seen_at}</code>
                                                </li>
                                            }
                                        }).collect_view();
                                        view! {
                                            <details class="proxy-preview-details">
                                                <summary>{format!("{} duplicates", duplicates_len)}</summary>
                                                <ul>{items}</ul>
                                            </details>
                                        }.into_any()
                                    } else { ().into_any() }}
                                    <p class="proxy-preview-hint muted">
                                        "Clicking Check Proxies will submit only the "
                                        <strong>{p.accepted.len()}</strong>
                                        " accepted proxies."
                                    </p>
                                </section>
                            }
                        })}

                        {move || input_error.get().map(|message| view! {
                            <div class="form-error" role="alert">{message}</div>
                        })}
                    </section>
                </div>

                <div class="proxy-sidebar-column">
                    <div class="proxy-quick-info">
                        <div class="proxy-quick-info-header">
                            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                            "Quick Info"
                        </div>
                        <div class="proxy-quick-info-item">
                            <span>"📊 Max Proxies"</span>
                            <strong>"500"</strong>
                        </div>
                        <div class="proxy-quick-info-item">
                            <span>"⏱ Timeout"</span>
                            <strong>"30s per proxy"</strong>
                        </div>
                        <div class="proxy-quick-info-item">
                            <span>"🌐 Protocol Support"</span>
                            <strong>"HTTP / HTTPS / SOCKS4 / SOCKS5"</strong>
                        </div>
                        <div class="proxy-quick-info-item">
                            <span>"🛡 SSRF Protection"</span>
                            <span class="proxy-badge-enabled">"Enabled"</span>
                        </div>
                    </div>

                    <div class="proxy-help-card">
                        <div class="proxy-help-icon">
                            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                        </div>
                        <h3 style="margin: 0 0 8px; font-size: 0.9rem; color: var(--pv-text);">"Need help?"</h3>
                        <p>"Check our documentation for more details about proxy checking."</p>
                        <button class="btn">"View Docs →"</button>
                    </div>
                </div>
            </div>

            // Progress Card
            {move || job.get().map(|current| {
                let status = current.status.clone();
                let progress = if current.total_count == 0 { 0 } else {
                    ((current.checked_count as f64 / current.total_count as f64) * 100.0).round().clamp(0.0, 100.0) as u32
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
                                    view! { <button class="btn btn-danger" type="button" on:click=cancel disabled=move || busy.get()>"Cancel"</button> }.into_any()
                                } else { ().into_any() }}
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
                    </section>
                }
            })}

            {move || timed_out.get().then(|| view! {
                <div class="error" role="status">"The UI stopped polling after 5 minutes. The job may still be running; use Refresh when you return."</div>
            })}

            // Summary & Results (Only show if a job exists)
            {move || if job.get().is_some() {
                let rows = results.get();
                let total = job.get().map(|j| j.total_count as usize).unwrap_or(rows.len());
                let working = rows.iter().filter(|r| r.result.as_ref().is_some_and(|b| b.is_alive())).count();
                let failed = rows.iter().filter(|r| r.result.as_ref().is_some_and(|b| !b.is_alive())).count();
                let https = rows.iter().filter(|r| r.result.as_ref().is_some_and(|b| b.supports_https)).count();
                let latency_values = rows.iter().filter_map(|r| r.result.as_ref().and_then(|b| b.latency_ms)).collect::<Vec<_>>();
                let avg_latency = if latency_values.is_empty() { "—".into() } else { format!("{} ms", latency_values.iter().copied().map(u64::from).sum::<u64>() / latency_values.len() as u64) };

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
                            <div><h2>"Results"</h2><span class="muted">{format!("{} loaded", rows.len())}</span></div>
                            <div class="proxy-export-actions">
                                <button class="btn" type="button" on:click=move |_| export(ResultFilter::All)>"Export All"</button>
                                <button class="btn" type="button" on:click=move |_| export(ResultFilter::Working)>"Export Working"</button>
                                <button class="btn" type="button" on:click=move |_| export(ResultFilter::Failed)>"Export Failed"</button>
                                <button class="btn" type="button" on:click=copy_working>"Copy Working"</button>
                            </div>
                        </div>

                        <div class="proxy-filters" role="toolbar" aria-label="Result filters">
                            {[ResultFilter::All, ResultFilter::Working, ResultFilter::TargetFailed, ResultFilter::ProxyFailed, ResultFilter::Failed, ResultFilter::Http, ResultFilter::Https, ResultFilter::Socks4, ResultFilter::Socks5, ResultFilter::HttpsCapable].into_iter().map(|filter| {
                                let label = filter.label();
                                view! {
                                    <button type="button" class:active=move || active_filter.get() == filter on:click=move |_| active_filter.set(filter)>{label}</button>
                                }
                            }).collect_view()}
                        </div>

                        <div class="proxy-result-grid">
                            {move || results.get().into_iter()
                                .filter(|row| matches_filter(row, active_filter.get()))
                                .map(|row| {
                                    let body = row.result.clone();
                                    // PART 3 — three-way status. Derive from
                                    // the effective_outcome so that both new
                                    // (PART 3) and legacy rows render correctly.
                                    let (status, status_class) = match body.as_ref().map(|r| r.effective_outcome()) {
                                        Some("working") => ("WORKING", "working"),
                                        Some("working_target_unreachable") => ("TARGET FAILED", "target-failed"),
                                        Some("proxy_failed") => ("PROXY FAILED", "failed"),
                                        _ => ("PENDING", "pending"),
                                    };
                                    let is_alive = body.as_ref().is_some_and(|r| r.is_alive());
                                    let total_latency = body.as_ref().and_then(|r| r.effective_total_time_ms()).map(|v| format!("{} ms", v)).unwrap_or_else(|| "—".into());
                                    let conn_latency = body.as_ref().and_then(|r| r.connection_latency_ms).map(|v| format!("{} ms", v)).unwrap_or_else(|| "—".into());
                                    let tls_latency = body.as_ref().and_then(|r| r.tls_handshake_latency_ms).map(|v| format!("{} ms", v)).unwrap_or_else(|| "—".into());
                                    let ttfb = body.as_ref().and_then(|r| r.ttfb_ms).map(|v| format!("{} ms", v)).unwrap_or_else(|| "—".into());
                                    let http_status = body.as_ref().and_then(|r| r.http_status_code).map(|v| v.to_string()).unwrap_or_else(|| "—".into());
                                    let https = body.as_ref().is_some_and(|r| r.supports_https);
                                    let country = body.as_ref().and_then(|r| r.country_code.clone()).unwrap_or_else(|| "—".into());
                                    let anonymity = body.as_ref().and_then(|r| r.anonymity.clone()).unwrap_or_else(|| "—".into());
                                    // Only show an error message when the proxy
                                    // actually failed OR when there's a target
                                    // warning. A fully working row shows
                                    // "Connection healthy".
                                    let error_text = match body.as_ref() {
                                        Some(r) if r.is_alive() && !r.is_fully_working() => {
                                            result_error(r)
                                        }
                                        Some(r) if !r.is_alive() => result_error(r),
                                        _ => "Connection healthy".into(),
                                    };
                                    let endpoint = format!("{}://{}:{}", row.scheme, row.host, row.port);
                                    let warning_class = if is_alive && !body.as_ref().is_some_and(|r| r.is_fully_working()) {
                                        "proxy-result-message proxy-result-message-warning"
                                    } else if is_alive {
                                        "proxy-result-message proxy-result-message-success"
                                    } else {
                                        "proxy-result-message proxy-result-message-error"
                                    };

                                    view! {
                                        <article class=format!("proxy-result-card proxy-result-{}", status_class)>
                                            <div class="proxy-result-top">
                                                <div class="proxy-result-status"><span class="proxy-status-dot"></span><strong>{status}</strong></div>
                                                <span class="proxy-result-scheme">{row.scheme.clone()}</span>
                                            </div>
                                            <div class="proxy-endpoint">
                                                <span class="proxy-endpoint-prefix">"PROXY"</span>
                                                <code>{endpoint}</code>
                                            </div>
                                            <div class="proxy-result-metrics proxy-result-metrics-wide">
                                                <div><span>"TOTAL"</span><strong>{total_latency}</strong></div>
                                                <div><span>"CONNECT"</span><strong>{conn_latency}</strong></div>
                                                <div><span>"TLS"</span><strong>{tls_latency}</strong></div>
                                                <div><span>"TTFB"</span><strong>{ttfb}</strong></div>
                                                <div><span>"HTTP"</span><strong>{http_status}</strong></div>
                                                <div><span>"HTTPS"</span><strong class:proxy-positive=https>{if https { "Supported" } else { "No" }}</strong></div>
                                                <div><span>"COUNTRY"</span><strong>{country}</strong></div>
                                                <div><span>"ANONYMITY"</span><strong>{anonymity}</strong></div>
                                            </div>
                                            <div class={warning_class}>
                                                <span>{if is_alive { "✓" } else { "!" }}</span>
                                                <span>{error_text}</span>
                                            </div>
                                        </article>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </section>
                }.into_any()
            } else {
                ().into_any()
            }}

            // Removed the static "Recent Results" table completely
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(reachable: bool, scheme: &str, https: bool) -> ProxyResult {
        // Legacy helper — leave `outcome` as None so tests exercise the
        // backward-compat derivation from `reachable` + `error_code`.
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
                outcome: None,
                failure_kind: None,
                connection_latency_ms: None,
                tls_handshake_latency_ms: None,
                ttfb_ms: None,
                total_time_ms: None,
                http_status_code: None,
            }),
        }
    }

    fn row_with_outcome(outcome: &str, error_code: Option<&str>) -> ProxyResult {
        ProxyResult {
            id: "id".into(),
            host: "8.8.8.8".into(),
            port: 8080,
            scheme: "http".into(),
            result: Some(ProxyResultBody {
                reachable: outcome == "working",
                latency_ms: Some(20),
                supports_https: false,
                country_code: None,
                anonymity: None,
                error_code: error_code.map(|s| s.into()),
                error_message: None,
                outcome: Some(outcome.into()),
                failure_kind: None,
                connection_latency_ms: Some(5),
                tls_handshake_latency_ms: None,
                ttfb_ms: Some(15),
                total_time_ms: Some(20),
                http_status_code: Some(200),
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

        let failed_socks4 = row(false, "socks4", false);
        assert!(matches_filter(&failed_socks4, ResultFilter::Socks4));
        assert!(!matches_filter(&failed_socks4, ResultFilter::Socks5));
    }

    #[test]
    fn target_failure_is_not_a_dead_proxy_in_filters() {
        // The central PART 3 invariant, exercised through the UI filter
        // logic: a target-side failure must appear under `Working`
        // (because the proxy is alive) AND under `TargetFailed`, but
        // NOT under `ProxyFailed`.
        let target_failed = row_with_outcome("working_target_unreachable", Some("TARGET_TIMEOUT"));

        assert!(matches_filter(&target_failed, ResultFilter::Working));
        assert!(matches_filter(&target_failed, ResultFilter::TargetFailed));
        assert!(!matches_filter(&target_failed, ResultFilter::ProxyFailed));

        let proxy_failed = row_with_outcome("proxy_failed", Some("CONNECTION_REFUSED"));
        assert!(!matches_filter(&proxy_failed, ResultFilter::Working));
        assert!(!matches_filter(&proxy_failed, ResultFilter::TargetFailed));
        assert!(matches_filter(&proxy_failed, ResultFilter::ProxyFailed));

        let working = row_with_outcome("working", None);
        assert!(matches_filter(&working, ResultFilter::Working));
        assert!(!matches_filter(&working, ResultFilter::TargetFailed));
        assert!(!matches_filter(&working, ResultFilter::ProxyFailed));
    }

    #[test]
    fn csv_export_uses_part3_columns() {
        let csv = result_to_csv(&[row(true, "http", true)]);
        assert!(csv.starts_with("host,port,scheme,outcome,"));
        assert!(csv.contains("connect_ms"));
        assert!(csv.contains("ttfb_ms"));
        assert!(csv.contains("http_status"));
        // Credentials must never appear.
        assert!(!csv.contains("username"));
        assert!(!csv.contains("password"));
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
        assert_eq!(human_error_code("PROXY_PRIVATE_IP_BLOCKED"), "This proxy points to a private or restricted address.");
        assert_eq!(human_error_code("JOB_FAILED"), "The proxy check could not be completed.");
    }
}
