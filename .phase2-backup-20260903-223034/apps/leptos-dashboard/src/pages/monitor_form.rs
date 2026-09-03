use crate::api::monitoring::{self, MonitorPayload};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

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
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    Effect::new(move |_| {
        if edit() {
            let mid = id();
            spawn_local(async move {
                match monitoring::get_monitor(&mid).await {
                    Ok(m) => {
                        name.set(m.name);
                        kind.set(m.kind);
                        url.set(m.url.unwrap_or_default());
                        method.set("GET".into());
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
                    }
                    Err(e) => error.set(Some(e)),
                }
            })
        }
    });

    // Use `move` to take ownership of all captured variables.
    let submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let payload = MonitorPayload {
            name: name.get(),
            kind: kind.get(),
            url: url.get(),
            method: method.get(),
            expected_status: expected.get().parse().unwrap_or(200),
            interval_seconds: interval.get().parse().unwrap_or(60),
            timeout_seconds: timeout.get().parse().unwrap_or(10),
            assertions: Vec::new(),
            is_public: public.get(),
            ssl_check_enabled: ssl.get(),
            ssl_expiry_warning_days: ssl_days.get().parse().unwrap_or(14),
            tcp_port: tcp_port.get().parse().ok(),
            dns_record_type: if kind.get() == "dns" {
                Some(dns_type.get())
            } else {
                None
            },
            dns_expected_value: if kind.get() == "dns" && !dns_value.get().is_empty() {
                Some(dns_value.get())
            } else {
                None
            },
            heartbeat_interval_seconds: if kind.get() == "heartbeat" {
                hb_interval.get().parse().ok()
            } else {
                None
            },
            heartbeat_grace_period_seconds: if kind.get() == "heartbeat" {
                hb_grace.get().parse().ok()
            } else {
                None
            },
        };
        let mid = id();
        let nav = navigate.clone();
        spawn_local(async move {
            let r = if edit() {
                monitoring::update_monitor(&mid, &payload).await
            } else {
                monitoring::create_monitor(&payload).await
            };
            match r {
                Ok(m) => nav(&format!("/monitors/{}", m.id), Default::default()),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>{move || if edit() { "Edit monitor" } else { "New monitor" }}</h1></header>
            <form on:submit=submit class="stack-form">
                <label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label>
                <label>"Kind"<select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))>
                    <option value="http">"HTTP"</option>
                    <option value="tcp">"TCP"</option>
                    <option value="dns">"DNS"</option>
                    <option value="heartbeat">"Heartbeat"</option>
                    <option value="flow">"API flow"</option>
                </select></label>
                {move || {
                    let k = kind.get();
                    match k.as_str() {
                        "http" | "flow" => {
                            view! {
                                <div>
                                    <label>"URL"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label>
                                    <label>"Method"<select prop:value=method on:change=move|e|method.set(event_target_value(&e))>
                                        <option value="GET">"GET"</option>
                                        <option value="HEAD">"HEAD"</option>
                                    </select></label>
                                    <label>"Expected status"<input type="number" min="100" max="599" prop:value=expected on:input=move|e|expected.set(event_target_value(&e))/></label>
                                </div>
                            }.into_any()
                        }
                        "tcp" => {
                            view! {
                                <div>
                                    <label>"Host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label>
                                    <label>"TCP port"<input type="number" min="1" max="65535" prop:value=tcp_port on:input=move|e|tcp_port.set(event_target_value(&e)) required/></label>
                                </div>
                            }.into_any()
                        }
                        "dns" => {
                            view! {
                                <div>
                                    <label>"DNS host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label>
                                    <label>"Record type"<input prop:value=dns_type on:input=move|e|dns_type.set(event_target_value(&e)) required/></label>
                                    <label>"Expected value (optional)"<input prop:value=dns_value on:input=move|e|dns_value.set(event_target_value(&e))/></label>
                                </div>
                            }.into_any()
                        }
                        "heartbeat" => {
                            view! {
                                <div>
                                    <label>"Heartbeat endpoint"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label>
                                    <label>"Heartbeat interval (seconds)"<input type="number" min="1" prop:value=hb_interval on:input=move|e|hb_interval.set(event_target_value(&e)) required/></label>
                                    <label>"Grace period (seconds)"<input type="number" min="1" prop:value=hb_grace on:input=move|e|hb_grace.set(event_target_value(&e)) required/></label>
                                </div>
                            }.into_any()
                        }
                        _ => ().into_any()
                    }
                }}
                <label>"Check interval (seconds)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e)) required/></label>
                <label>"Timeout (seconds)"<input type="number" min="1" prop:value=timeout on:input=move|e|timeout.set(event_target_value(&e)) required/></label>
                <label class="check-row"><input type="checkbox" prop:checked=public on:change=move|e|public.set(event_target::<web_sys::HtmlInputElement>(&e).checked())/>"Show on public status pages"</label>
                <label class="check-row"><input type="checkbox" prop:checked=ssl on:change=move|e|ssl.set(event_target::<web_sys::HtmlInputElement>(&e).checked())/>"Check TLS certificate expiry"</label>
                {move || if ssl.get() {
                    view! {
                        <label>"Expiry warning (days)"<input type="number" min="1" prop:value=ssl_days on:input=move|e|ssl_days.set(event_target_value(&e))/></label>
                    }.into_any()
                } else {
                    ().into_any()
                }}
                <button class="btn btn-primary" type="submit">{move || if edit() { "Save changes" } else { "Create monitor" }}</button>
                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}
            </form>
        </div>
    }
}
