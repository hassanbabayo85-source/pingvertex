use crate::api::monitoring::{self, MonitorPayload};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

fn parse_u32_field(value: &str, field: &str, min: u32, max: Option<u32>) -> Result<u32, String> {
    let parsed = value
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("{field} must be a valid number."))?;
    if parsed < min || max.is_some_and(|limit| parsed > limit) {
        return Err(match max {
            Some(limit) => format!("{field} must be between {min} and {limit}."),
            None => format!("{field} must be at least {min}."),
        });
    }
    Ok(parsed)
}

#[component]
pub fn MonitorFormPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();
    let edit = move || !id().is_empty();

    let name = RwSignal::new(String::new());
    let kind = RwSignal::new("http".into());
    let url = RwSignal::new(String::new());
    let method = RwSignal::new("GET".into());
    let expected = RwSignal::new("200".into());
    let tcp_port = RwSignal::new(String::new());
    let dns_type = RwSignal::new("A".into());
    let dns_value = RwSignal::new(String::new());
    let hb_interval = RwSignal::new("60".into());
    let hb_grace = RwSignal::new("60".into());
    let interval = RwSignal::new("60".into());
    let timeout = RwSignal::new("10".into());
    let public = RwSignal::new(false);
    let ssl = RwSignal::new(true);
    let ssl_days = RwSignal::new("14".into());
    let assertions = RwSignal::new(Vec::<serde_json::Value>::new());

    let loading = RwSignal::new(edit());
    let submitting = RwSignal::new(false);
    let load_error = RwSignal::new(None::<String>);
    let success = RwSignal::new(None::<String>);

    let name_error = RwSignal::new(None::<String>);
    let target_error = RwSignal::new(None::<String>);
    let expected_error = RwSignal::new(None::<String>);
    let interval_error = RwSignal::new(None::<String>);
    let timeout_error = RwSignal::new(None::<String>);
    let tcp_error = RwSignal::new(None::<String>);
    let dns_error = RwSignal::new(None::<String>);
    let heartbeat_error = RwSignal::new(None::<String>);
    let ssl_error = RwSignal::new(None::<String>);

    let navigate = use_navigate();

    Effect::new(move |_| {
        if !edit() {
            loading.set(false);
            return;
        }

        let monitor_id = id();
        spawn_local(async move {
            match monitoring::get_monitor(&monitor_id).await {
                Ok(m) => {
                    name.set(m.name);
                    kind.set(m.kind.to_lowercase());
                    url.set(m.url.unwrap_or_default());
                    method.set(m.method);
                    expected.set(m.expected_status.to_string());
                    tcp_port.set(m.tcp_port.map(|v| v.to_string()).unwrap_or_default());
                    dns_type.set(m.dns_record_type.unwrap_or_else(|| "A".into()));
                    dns_value.set(m.dns_expected_value.unwrap_or_default());
                    hb_interval.set(m.heartbeat_interval_seconds.unwrap_or(60).to_string());
                    hb_grace.set(m.heartbeat_grace_period_seconds.unwrap_or(60).to_string());
                    interval.set(m.interval_seconds.to_string());
                    timeout.set(m.timeout_seconds.to_string());
                    public.set(m.is_public);
                    ssl.set(m.ssl_check_enabled);
                    ssl_days.set(m.ssl_expiry_warning_days.to_string());
                    // Preserve existing assertions. The previous form always sent an
                    // empty list during edits, which silently deleted configured assertions.
                    assertions.set(m.assertions);
                    loading.set(false);
                }
                Err(e) => {
                    load_error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    let clear_validation = move || {
        name_error.set(None);
        target_error.set(None);
        expected_error.set(None);
        interval_error.set(None);
        timeout_error.set(None);
        tcp_error.set(None);
        dns_error.set(None);
        heartbeat_error.set(None);
        ssl_error.set(None);
        load_error.set(None);
    };

    let submit = Callback::new(move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        clear_validation();

        if submitting.get() || loading.get() {
            return;
        }

        let kind_value = kind.get().trim().to_lowercase();
        let valid_kind = matches!(
            kind_value.as_str(),
            "http" | "tcp" | "dns" | "heartbeat" | "flow"
        );
        if !valid_kind {
            name_error.set(Some("Choose a supported monitor type.".into()));
            return;
        }

        let name_value = name.get().trim().to_string();
        if name_value.is_empty() {
            name_error.set(Some("Monitor name is required.".into()));
            return;
        }
        if name_value.chars().count() > 200 {
            name_error.set(Some("Monitor name must be 200 characters or fewer.".into()));
            return;
        }

        let target = url.get().trim().to_string();
        if target.is_empty() {
            target_error.set(Some(match kind_value.as_str() {
                "tcp" | "dns" => "Host is required.".into(),
                _ => "URL is required.".into(),
            }));
            return;
        }

        let expected_status =
            match parse_u32_field(&expected.get(), "Expected status", 100, Some(599)) {
                Ok(v) => v as u16,
                Err(e) => {
                    expected_error.set(Some(e));
                    return;
                }
            };

        let interval_seconds = match parse_u32_field(&interval.get(), "Check interval", 1, None) {
            Ok(v) => v,
            Err(e) => {
                interval_error.set(Some(e));
                return;
            }
        };

        let timeout_seconds = match parse_u32_field(&timeout.get(), "Timeout", 1, None) {
            Ok(v) => v,
            Err(e) => {
                timeout_error.set(Some(e));
                return;
            }
        };

        let tcp_port_value = if kind_value == "tcp" {
            match parse_u32_field(&tcp_port.get(), "TCP port", 1, Some(65535)) {
                Ok(v) => Some(v as u16),
                Err(e) => {
                    tcp_error.set(Some(e));
                    return;
                }
            }
        } else {
            None
        };

        let (heartbeat_interval_seconds, heartbeat_grace_period_seconds) =
            if kind_value == "heartbeat" {
                let heartbeat_interval =
                    match parse_u32_field(&hb_interval.get(), "Heartbeat interval", 1, None) {
                        Ok(v) => v,
                        Err(e) => {
                            heartbeat_error.set(Some(e));
                            return;
                        }
                    };
                let heartbeat_grace =
                    match parse_u32_field(&hb_grace.get(), "Heartbeat grace period", 1, None) {
                        Ok(v) => v,
                        Err(e) => {
                            heartbeat_error.set(Some(e));
                            return;
                        }
                    };
                (Some(heartbeat_interval), Some(heartbeat_grace))
            } else {
                (None, None)
            };

        let dns_record_type = if kind_value == "dns" {
            let record_type = dns_type.get().trim().to_uppercase();
            if !matches!(
                record_type.as_str(),
                "A" | "AAAA" | "CNAME" | "TXT" | "MX" | "NS"
            ) {
                dns_error.set(Some(
                    "Use a supported DNS record type: A, AAAA, CNAME, TXT, MX, or NS.".into(),
                ));
                return;
            }
            Some(record_type)
        } else {
            None
        };

        let dns_expected_value = if kind_value == "dns" && !dns_value.get().trim().is_empty() {
            Some(dns_value.get().trim().to_string())
        } else {
            None
        };

        let ssl_expiry_warning_days = if ssl.get() {
            match parse_u32_field(&ssl_days.get(), "SSL warning days", 1, Some(3650)) {
                Ok(v) => v,
                Err(e) => {
                    ssl_error.set(Some(e));
                    return;
                }
            }
        } else {
            // Explicitly send the existing/default value even when SSL checks are disabled.
            match parse_u32_field(&ssl_days.get(), "SSL warning days", 1, Some(3650)) {
                Ok(v) => v,
                Err(e) => {
                    ssl_error.set(Some(e));
                    return;
                }
            }
        };

        let payload = MonitorPayload {
            name: name_value,
            kind: kind_value,
            url: target,
            method: method.get(),
            expected_status,
            interval_seconds,
            timeout_seconds,
            assertions: assertions.get(),
            is_public: public.get(),
            ssl_check_enabled: ssl.get(),
            ssl_expiry_warning_days,
            tcp_port: tcp_port_value,
            dns_record_type,
            dns_expected_value,
            heartbeat_interval_seconds,
            heartbeat_grace_period_seconds,
        };

        let monitor_id = id();
        let is_edit = edit();
        submitting.set(true);

        let nav = navigate.clone();
        spawn_local(async move {
            let result = if is_edit {
                monitoring::update_monitor(&monitor_id, &payload).await
            } else {
                monitoring::create_monitor(&payload).await
            };

            submitting.set(false);
            match result {
                Ok(monitor) => {
                    success.set(Some(if is_edit {
                        "Monitor updated successfully.".into()
                    } else {
                        "Monitor created successfully.".into()
                    }));
                    let suffix = if is_edit { "?saved=1" } else { "?created=1" };
                    nav(
                        &format!("/monitors/{}{}", monitor.id, suffix),
                        Default::default(),
                    );
                }
                Err(e) => load_error.set(Some(e)),
            }
        });
    });

    view! {
        <div class="feature-page monitor-form-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"Monitor management"</span>
                    <h1>{move || if edit() { "Edit monitor" } else { "New monitor" }}</h1>
                </div>
            </header>

            {move || if loading.get() {
                view! {
                    <section class="data-section">
                        <div class="loading">"Loading monitor settings…"</div>
                    </section>
                }.into_any()
            } else if let Some(e) = load_error.get() {
                view! {
                    <section class="data-section">
                        <div class="alert alert-error" role="alert">{e}</div>
                    </section>
                }.into_any()
            } else {
                let submit_cb = submit.clone();
                view! {
                    <form on:submit=submit_cb class="stack-form monitor-form">
                        <section class="data-section form-section">
                            <h2>"Identity"</h2>
                            <label>
                                "Name"
                                <input
                                    prop:value=name
                                    on:input=move |e| {
                                        name.set(event_target_value(&e));
                                        name_error.set(None);
                                    }
                                    maxlength="200"
                                    autocomplete="off"
                                    required
                                />
                            </label>
                            {move || name_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}

                            <label>
                                "Monitor type"
                                <select prop:value=kind on:change=move |e| {
                                    kind.set(event_target_value(&e));
                                    target_error.set(None);
                                }>
                                    <option value="http">"HTTP"</option>
                                    <option value="tcp">"TCP"</option>
                                    <option value="dns">"DNS"</option>
                                    <option value="heartbeat">"Heartbeat"</option>
                                    <option value="flow">"API flow"</option>
                                </select>
                            </label>
                            <p class="form-help">"Type-specific fields are validated before submission and by the backend."</p>
                        </section>

                        {move || {
                            match kind.get().as_str() {
                                "http" | "flow" => view! {
                                    <section class="data-section form-section">
                                        <h2>"HTTP target"</h2>
                                        <label>
                                            "URL"
                                            <input
                                                type="url"
                                                prop:value=url
                                                on:input=move |e| {
                                                    url.set(event_target_value(&e));
                                                    target_error.set(None);
                                                }
                                                autocomplete="url"
                                                maxlength="2048"
                                                required
                                            />
                                        </label>
                                        {move || target_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                        <label>
                                            "Method"
                                            <select prop:value=method on:change=move |e| method.set(event_target_value(&e))>
                                                <option value="GET">"GET"</option>
                                                <option value="HEAD">"HEAD"</option>
                                            </select>
                                        </label>
                                        <label>
                                            "Expected status"
                                            <input type="number" min="100" max="599" prop:value=expected on:input=move |e| {
                                                expected.set(event_target_value(&e));
                                                expected_error.set(None);
                                            }/>
                                        </label>
                                        {move || expected_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                    </section>
                                }.into_any(),
                                "tcp" => view! {
                                    <section class="data-section form-section">
                                        <h2>"TCP target"</h2>
                                        <label>
                                            "Host"
                                            <input
                                                type="text"
                                                prop:value=url
                                                on:input=move |e| {
                                                    url.set(event_target_value(&e));
                                                    target_error.set(None);
                                                }
                                                maxlength="253"
                                                required
                                            />
                                        </label>
                                        {move || target_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                        <label>
                                            "TCP port"
                                            <input type="number" min="1" max="65535" prop:value=tcp_port on:input=move |e| {
                                                tcp_port.set(event_target_value(&e));
                                                tcp_error.set(None);
                                            } required/>
                                        </label>
                                        {move || tcp_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                    </section>
                                }.into_any(),
                                "dns" => view! {
                                    <section class="data-section form-section">
                                        <h2>"DNS target"</h2>
                                        <label>
                                            "Host"
                                            <input type="text" prop:value=url maxlength="253" on:input=move |e| {
                                                url.set(event_target_value(&e));
                                                target_error.set(None);
                                            } required/>
                                        </label>
                                        {move || target_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                        <label>
                                            "Record type"
                                            <select prop:value=dns_type on:change=move |e| {
                                                dns_type.set(event_target_value(&e));
                                                dns_error.set(None);
                                            }>
                                                <option value="A">"A"</option>
                                                <option value="AAAA">"AAAA"</option>
                                                <option value="CNAME">"CNAME"</option>
                                                <option value="TXT">"TXT"</option>
                                                <option value="MX">"MX"</option>
                                                <option value="NS">"NS"</option>
                                            </select>
                                        </label>
                                        {move || dns_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                        <label>
                                            "Expected value (optional)"
                                            <input type="text" maxlength="2048" prop:value=dns_value on:input=move |e| dns_value.set(event_target_value(&e))/>
                                        </label>
                                    </section>
                                }.into_any(),
                                "heartbeat" => view! {
                                    <section class="data-section form-section">
                                        <h2>"Heartbeat target"</h2>
                                        <label>
                                            "Endpoint"
                                            <input type="url" prop:value=url maxlength="2048" on:input=move |e| {
                                                url.set(event_target_value(&e));
                                                target_error.set(None);
                                            } required/>
                                        </label>
                                        {move || target_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                        <label>
                                            "Heartbeat interval (seconds)"
                                            <input type="number" min="1" prop:value=hb_interval on:input=move |e| {
                                                hb_interval.set(event_target_value(&e));
                                                heartbeat_error.set(None);
                                            } required/>
                                        </label>
                                        <label>
                                            "Grace period (seconds)"
                                            <input type="number" min="1" prop:value=hb_grace on:input=move |e| {
                                                hb_grace.set(event_target_value(&e));
                                                heartbeat_error.set(None);
                                            } required/>
                                        </label>
                                        {move || heartbeat_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                    </section>
                                }.into_any(),
                                _ => ().into_any(),
                            }
                        }}

                        <section class="data-section form-section">
                            <h2>"Schedule & reliability"</h2>
                            <label>
                                "Check interval (seconds)"
                                <input type="number" min="1" prop:value=interval on:input=move |e| {
                                    interval.set(event_target_value(&e));
                                    interval_error.set(None);
                                } required/>
                            </label>
                            {move || interval_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                            <label>
                                "Timeout (seconds)"
                                <input type="number" min="1" prop:value=timeout on:input=move |e| {
                                    timeout.set(event_target_value(&e));
                                    timeout_error.set(None);
                                } required/>
                            </label>
                            {move || timeout_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                        </section>

                        <section class="data-section form-section">
                            <h2>"Visibility & TLS"</h2>
                            <label class="check-row">
                                <input type="checkbox" prop:checked=public on:change=move |e| {
                                    public.set(event_target::<web_sys::HtmlInputElement>(&e).checked());
                                }/>
                                "Show on public status pages"
                            </label>
                            <label class="check-row">
                                <input type="checkbox" prop:checked=ssl on:change=move |e| {
                                    ssl.set(event_target::<web_sys::HtmlInputElement>(&e).checked());
                                }/>
                                "Check TLS certificate expiry"
                            </label>
                            {move || if ssl.get() {
                                view! {
                                    <label>
                                        "Expiry warning (days)"
                                        <input type="number" min="1" max="3650" prop:value=ssl_days on:input=move |e| {
                                            ssl_days.set(event_target_value(&e));
                                            ssl_error.set(None);
                                        }/>
                                    </label>
                                    {move || ssl_error.get().map(|e| view! { <div class="form-error-message">{e}</div> })}
                                }.into_any()
                            } else {
                                ().into_any()
                            }}
                            <p class="form-help">"Existing assertions are preserved when editing; this form does not silently clear them."</p>
                        </section>

                        {move || success.get().map(|message| view! {
                            <div class="alert alert-success" role="status" aria-live="polite">{message}</div>
                        })}
                        {move || load_error.get().map(|e| view! {
                            <div class="alert alert-error" role="alert">{e}</div>
                        })}

                        <div class="form-actions">
                            <button class="btn btn-primary" type="submit" disabled=move || submitting.get()>
                                {move || if submitting.get() { "Saving…" } else if edit() { "Save changes" } else { "Create monitor" }}
                            </button>
                        </div>
                    </form>
                }.into_any()
            }}
        </div>
    }
}
