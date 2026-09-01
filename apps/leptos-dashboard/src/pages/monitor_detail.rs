use crate::api::monitoring::{self, ChannelPayload, Check, Incident, Monitor, NotificationChannel};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

#[component]
pub fn MonitorDetailPage() -> impl IntoView {
    let params = use_params_map();
    let monitor_id = move || params.get().get("id").unwrap_or_default();
    let monitor = RwSignal::new(Monitor::default());
    let checks = RwSignal::new(Vec::<Check>::new());
    let incidents = RwSignal::new(Vec::<Incident>::new());
    let channels = RwSignal::new(Vec::<NotificationChannel>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    let load_data = move || {
        let id = monitor_id();
        if id.is_empty() {
            return;
        }
        spawn_local(async move {
            let monitor_fut = monitoring::get_monitor(&id);
            let checks_fut = monitoring::fetch_checks(&id);
            let incidents_fut = monitoring::fetch_incidents(&id);
            let channels_fut = monitoring::list_notification_channels(&id);

            match monitor_fut.await {
                Ok(m) => monitor.set(m),
                Err(e) => error.set(Some(e)),
            }
            match checks_fut.await {
                Ok(c) => checks.set(c),
                Err(e) => error.set(Some(e)),
            }
            match incidents_fut.await {
                Ok(i) => incidents.set(i),
                Err(e) => error.set(Some(e)),
            }
            match channels_fut.await {
                Ok(ch) => channels.set(ch),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| {
        load_data();
    });

    let navigate_for_delete = navigate.clone();
    let delete_monitor = move |_| {
        let id = monitor_id();
        let navigate = navigate_for_delete.clone();
        spawn_local(async move {
            if let Err(e) = monitoring::delete_monitor(&id).await {
                error.set(Some(e));
            } else {
                navigate("/monitors", Default::default());
            }
        });
    };

    let toggle_enabled = move |_| {
        let id = monitor_id();
        let current = monitor.get().enabled;
        spawn_local(async move {
            if let Err(e) = monitoring::set_monitor_enabled(&id, !current).await {
                error.set(Some(e));
            } else {
                load_data();
            }
        });
    };

    let ch_name = RwSignal::new(String::new());
    let ch_kind = RwSignal::new(String::new());
    let ch_target = RwSignal::new(String::new());
    let channel_error = RwSignal::new(None::<String>);

    let create_channel = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let id = monitor_id();
        let payload = ChannelPayload {
            name: ch_name.get(),
            kind: ch_kind.get(),
            target: ch_target.get(),
        };
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &payload).await {
                Ok(_) => {
                    ch_name.set(String::new());
                    ch_kind.set(String::new());
                    ch_target.set(String::new());
                    load_data();
                }
                Err(e) => channel_error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="monitor-detail">
            {move || {
                if loading.get() {
                    view! { <div class="loading">"Loading monitor..."</div> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <div class="error">{err}</div> }.into_any()
                } else {
                    let m = monitor.get();
                    let navigate_edit = navigate.clone();
                    view! {
                        <header class="page-header">
                            <h1>{m.name.clone()}</h1>
                            <div>
                                <button on:click=move |_| navigate_edit(&format!("/monitors/{}/edit", monitor_id()), Default::default())>
                                    "Edit"
                                </button>
                                <button on:click=delete_monitor.clone()>"Delete"</button>
                                <button on:click=toggle_enabled>
                                    {if m.enabled { "Disable" } else { "Enable" }}
                                </button>
                            </div>
                        </header>
                        <div class="monitor-info">
                            <p><strong>"Kind: "</strong>{m.kind.clone()}</p>
                            <p><strong>"Status: "</strong>{m.status.clone().unwrap_or_else(|| "unknown".to_string())}</p>
                            <p><strong>"URL: "</strong>{m.url.clone().unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Host: "</strong>{m.host.clone().unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Port: "</strong>{m.port.map(|v| v.to_string()).unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Interval (s): "</strong>{m.interval_seconds}</p>
                            <p><strong>"Timeout (s): "</strong>{m.timeout_seconds}</p>
                        </div>

                        <h2>"Recent Checks"</h2>
                        <div class="checks-list">
                            {if checks.get().is_empty() {
                                view! { <p>"No checks recorded."</p> }.into_any()
                            } else {
                                checks.get().into_iter().take(20).map(|c| {
                                    view! {
                                        <div class="check-item">
                                            <span>{c.checked_at}</span>
                                            <span>{c.status}</span>
                                            <span>{c.response_time_ms.map(|v| format!("{v} ms")).unwrap_or_else(|| "—".into())}</span>
                                            <span>{c.region.clone().unwrap_or_default()}</span>
                                        </div>
                                    }.into_any()
                                }).collect_view().into_any()
                            }}
                        </div>

                        <h2>"Incidents"</h2>
                        <div class="incidents-list">
                            {if incidents.get().is_empty() {
                                view! { <p>"No incidents."</p> }.into_any()
                            } else {
                                incidents.get().into_iter().map(|inc| {
                                    view! {
                                        <div class="incident-item">
                                            <span>{inc.started_at}</span>
                                            <span>{inc.title}</span>
                                            <span>{inc.status}</span>
                                            <span>{inc.resolved_at.unwrap_or_else(|| "—".into())}</span>
                                        </div>
                                    }.into_any()
                                }).collect_view().into_any()
                            }}
                        </div>

                        <h2>"Notification Channels"</h2>
                        <div class="channels-list">
                            {if channels.get().is_empty() {
                                view! { <p>"No channels configured."</p> }.into_any()
                            } else {
                                channels.get().into_iter().map(|ch| {
                                    let id = ch.id.clone();
                                    let delete_channel = move |_| {
                                        let id = id.clone();
                                        spawn_local(async move {
                                            if let Err(e) = monitoring::delete_notification_channel(&id).await {
                                                error.set(Some(e));
                                            } else {
                                                load_data();
                                            }
                                        });
                                    };
                                    view! {
                                        <div class="channel-item">
                                            <span>{ch.name}</span>
                                            <span>{ch.kind}</span>
                                            <span>{ch.target}</span>
                                            <button on:click=delete_channel.clone()>"Delete"</button>
                                        </div>
                                    }.into_any()
                                }).collect_view().into_any()
                            }}
                        </div>

                        <form on:submit=create_channel class="channel-form">
                            <input
                                type="text"
                                placeholder="Channel name"
                                prop:value=ch_name
                                on:input=move |ev| ch_name.set(event_target_value(&ev))
                                required
                            />
                            <select
                                prop:value=ch_kind
                                on:change=move |ev| ch_kind.set(event_target_value(&ev))
                            >
                                <option value="">"Select type"</option>
                                <option value="email">"Email"</option>
                                <option value="webhook">"Webhook"</option>
                                <option value="sms">"SMS"</option>
                            </select>
                            <input
                                type="text"
                                placeholder="Target (email, URL, phone)"
                                prop:value=ch_target
                                on:input=move |ev| ch_target.set(event_target_value(&ev))
                                required
                            />
                            <button type="submit">"Add Channel"</button>
                        </form>
                        {move || channel_error.get().map(|e| view! { <div class="error">{e}</div> })}
                    }.into_any()
                }
            }}
        </div>
    }
}
