use crate::api::monitoring::{self, MonitorPayload};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

#[component]
pub fn MonitorFormPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();
    let is_edit = move || !id().is_empty();

    let name = RwSignal::new(String::new());
    let kind = RwSignal::new(String::from("http"));
    let url = RwSignal::new(String::new());
    let host = RwSignal::new(String::new());
    let port = RwSignal::new(String::new());
    let interval_seconds = RwSignal::new(String::from("60"));
    let timeout_seconds = RwSignal::new(String::from("10"));
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    Effect::new(move |_| {
        if is_edit() {
            let monitor_id = id();
            spawn_local(async move {
                match monitoring::get_monitor(&monitor_id).await {
                    Ok(m) => {
                        name.set(m.name);
                        kind.set(m.kind);
                        url.set(m.url.unwrap_or_default());
                        host.set(m.host.unwrap_or_default());
                        port.set(m.port.map(|p| p.to_string()).unwrap_or_default());
                        interval_seconds.set(m.interval_seconds.to_string());
                        timeout_seconds.set(m.timeout_seconds.to_string());
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let payload = MonitorPayload {
            name: name.get(),
            kind: kind.get(),
            url: if url.get().is_empty() {
                None
            } else {
                Some(url.get())
            },
            host: if host.get().is_empty() {
                None
            } else {
                Some(host.get())
            },
            port: port.get().parse::<u16>().ok(),
            interval_seconds: interval_seconds.get().parse::<u64>().unwrap_or(60),
            timeout_seconds: timeout_seconds.get().parse::<u64>().unwrap_or(10),
        };
        let navigate = navigate.clone();
        spawn_local(async move {
            if is_edit() {
                match monitoring::update_monitor(&id(), &payload).await {
                    Ok(_) => navigate(&format!("/monitors/{}", id()), Default::default()),
                    Err(e) => error.set(Some(e)),
                }
            } else {
                match monitoring::create_monitor(&payload).await {
                    Ok(m) => navigate(&format!("/monitors/{}", m.id), Default::default()),
                    Err(e) => error.set(Some(e)),
                }
            }
        });
    };

    view! {
        <div class="monitor-form-page">
            <header class="page-header">
                <h1>{if is_edit() { "Edit Monitor" } else { "New Monitor" }}</h1>
            </header>
            <form on:submit=on_submit>
                <div>
                    <label for="name">"Name"</label>
                    <input
                        type="text"
                        id="name"
                        prop:value=name
                        on:input=move |ev| name.set(event_target_value(&ev))
                        required
                    />
                </div>
                <div>
                    <label for="kind">"Kind"</label>
                    <select
                        id="kind"
                        prop:value=kind
                        on:change=move |ev| kind.set(event_target_value(&ev))
                    >
                        <option value="http">"HTTP"</option>
                        <option value="tcp">"TCP"</option>
                        <option value="heartbeat">"Heartbeat"</option>
                    </select>
                </div>
                <div>
                    <label for="url">"URL (for HTTP)"</label>
                    <input
                        type="text"
                        id="url"
                        prop:value=url
                        on:input=move |ev| url.set(event_target_value(&ev))
                    />
                </div>
                <div>
                    <label for="host">"Host (for TCP/heartbeat)"</label>
                    <input
                        type="text"
                        id="host"
                        prop:value=host
                        on:input=move |ev| host.set(event_target_value(&ev))
                    />
                </div>
                <div>
                    <label for="port">"Port (for TCP)"</label>
                    <input
                        type="number"
                        id="port"
                        prop:value=port
                        on:input=move |ev| port.set(event_target_value(&ev))
                    />
                </div>
                <div>
                    <label for="interval">"Interval (seconds)"</label>
                    <input
                        type="number"
                        id="interval"
                        prop:value=interval_seconds
                        on:input=move |ev| interval_seconds.set(event_target_value(&ev))
                        required
                    />
                </div>
                <div>
                    <label for="timeout">"Timeout (seconds)"</label>
                    <input
                        type="number"
                        id="timeout"
                        prop:value=timeout_seconds
                        on:input=move |ev| timeout_seconds.set(event_target_value(&ev))
                        required
                    />
                </div>
                <button type="submit">{if is_edit() { "Save Changes" } else { "Create Monitor" }}</button>
                {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            </form>
        </div>
    }
}
