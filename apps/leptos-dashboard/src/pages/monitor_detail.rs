use crate::api::monitoring::{self, ChannelPayload, Check, Incident, Monitor, NotificationChannel};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::empty_state::EmptyState;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::PageHeader;
use crate::components::status_badge::StatusBadge;
use crate::components::table_shell::TableShell;
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

    let show_delete_confirm = RwSignal::new(false);

    let navigate_for_delete = navigate.clone();
    let confirm_delete_cb = Callback::new(move |_: ()| {
        let id = monitor_id();
        let navigate = navigate_for_delete.clone();
        spawn_local(async move {
            if let Err(e) = monitoring::delete_monitor(&id).await {
                error.set(Some(e));
            } else {
                navigate("/monitors", Default::default());
            }
        });
        show_delete_confirm.set(false);
    });
    let cancel_delete_cb = Callback::new(move |_: ()| {
        show_delete_confirm.set(false);
    });

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

    let acknowledge = move |incident_id: String| {
        spawn_local(async move {
            if let Err(e) = monitoring::acknowledge_incident(&incident_id).await {
                error.set(Some(e));
            } else {
                load_data();
            }
        });
    };

    let send_test = move |channel_id: String| {
        spawn_local(async move {
            if let Err(e) = monitoring::test_notification_channel(&channel_id).await {
                error.set(Some(e));
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
                    let checks_count = checks.get().len();
                    let incidents_count = incidents.get().len();
                    let uptime_display = m
                        .uptime_percentage
                        .map(|v| format!("{v:.2}%"))
                        .unwrap_or_else(|| "—".to_string());
                    let avg_response_display = m
                        .avg_response_time_ms
                        .map(|v| format!("{v:.0} ms"))
                        .unwrap_or_else(|| "—".to_string());

                    view! {
                        <PageHeader title=m.name.clone()>
                            <button on:click=move |_| navigate_edit(&format!("/monitors/{}/edit", monitor_id()), Default::default())>
                                "Edit"
                            </button>
                            <button on:click=toggle_enabled>
                                {if m.enabled { "Disable" } else { "Enable" }}
                            </button>
                            <button class="btn btn-danger" on:click=move |_| show_delete_confirm.set(true)>
                                "Delete"
                            </button>
                        </PageHeader>

                        {move || {
                            if show_delete_confirm.get() {
                                view! {
                                    <ConfirmDialog
                                        title="Delete monitor?"
                                        body="This will permanently delete this monitor and its history. This cannot be undone."
                                        on_confirm=confirm_delete_cb
                                        on_cancel=cancel_delete_cb
                                    />
                                }
                                    .into_any()
                            } else {
                                ().into_any()
                            }
                        }}

                        <div class="monitor-info">
                            <p><strong>"Kind: "</strong>{m.kind.clone()}</p>
                            <p><strong>"Status: "</strong>{m.status.clone().unwrap_or_else(|| "unknown".to_string())}</p>
                            <p><strong>"URL: "</strong>{m.url.clone().unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Host: "</strong>{m.host.clone().unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Port: "</strong>{m.port.map(|v| v.to_string()).unwrap_or_else(|| "—".into())}</p>
                            <p><strong>"Interval (s): "</strong>{m.interval_seconds}</p>
                            <p><strong>"Timeout (s): "</strong>{m.timeout_seconds}</p>
                        </div>

                        <div class="metric-strip">
                            <MetricCard label="Uptime" value=uptime_display/>
                            <MetricCard label="Avg Response" value=avg_response_display/>
                            <MetricCard label="Checks" value=checks_count.to_string()/>
                            <MetricCard label="Incidents" value=incidents_count.to_string()/>
                        </div>

                        <h2>"Recent Checks"</h2>
                        {move || {
                            if checks.get().is_empty() {
                                view! {
                                    <EmptyState
                                        title="No checks recorded"
                                        description="Checks will appear here once this monitor runs."
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Checked At".to_string(),
                                        "Status".to_string(),
                                        "Response Time".to_string(),
                                        "Region".to_string(),
                                    ]>
                                        {checks
                                            .get()
                                            .into_iter()
                                            .take(20)
                                            .map(|c| {
                                                view! {
                                                    <tr>
                                                        <td>{c.checked_at}</td>
                                                        <td><StatusBadge status=c.status/></td>
                                                        <td>
                                                            {c
                                                                .response_time_ms
                                                                .map(|v| format!("{v} ms"))
                                                                .unwrap_or_else(|| "—".into())}
                                                        </td>
                                                        <td>{c.region.clone().unwrap_or_default()}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </TableShell>
                                }
                                    .into_any()
                            }
                        }}

                        <h2>"Incidents"</h2>
                        {move || {
                            if incidents.get().is_empty() {
                                view! {
                                    <EmptyState title="No incidents" description="This monitor has no recorded incidents."/>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Started".to_string(),
                                        "Title".to_string(),
                                        "Status".to_string(),
                                        "Resolved".to_string(),
                                        "Actions".to_string(),
                                    ]>
                                        {incidents
                                            .get()
                                            .into_iter()
                                            .map(|inc| {
                                                let inc_id = inc.id.clone();
                                                let status_lower = inc.status.to_lowercase();
                                                let can_acknowledge = status_lower != "resolved"
                                                    && status_lower != "acknowledged";
                                                view! {
                                                    <tr>
                                                        <td>{inc.started_at}</td>
                                                        <td>{inc.title}</td>
                                                        <td><StatusBadge status=inc.status.clone()/></td>
                                                        <td>{inc.resolved_at.clone().unwrap_or_else(|| "—".into())}</td>
                                                        <td>
                                                            {if can_acknowledge {
                                                                let inc_id = inc_id.clone();
                                                                view! {
                                                                    <button on:click=move |_| acknowledge(inc_id.clone())>
                                                                        "Acknowledge"
                                                                    </button>
                                                                }
                                                                    .into_any()
                                                            } else {
                                                                ().into_any()
                                                            }}
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </TableShell>
                                }
                                    .into_any()
                            }
                        }}

                        <h2>"Notifications"</h2>
                        {move || {
                            if channels.get().is_empty() {
                                view! {
                                    <EmptyState
                                        title="No notifications configured"
                                        description="Add a notification below to be alerted when this monitor changes status."
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Name".to_string(),
                                        "Kind".to_string(),
                                        "Target".to_string(),
                                        "Actions".to_string(),
                                    ]>
                                        {channels
                                            .get()
                                            .into_iter()
                                            .map(|ch| {
                                                let del_id = ch.id.clone();
                                                let test_id = ch.id.clone();
                                                let delete_channel = move |_| {
                                                    let del_id = del_id.clone();
                                                    spawn_local(async move {
                                                        if let Err(e) = monitoring::delete_notification_channel(&del_id).await {
                                                            error.set(Some(e));
                                                        } else {
                                                            load_data();
                                                        }
                                                    });
                                                };
                                                view! {
                                                    <tr>
                                                        <td>{ch.name}</td>
                                                        <td>{ch.kind}</td>
                                                        <td>{ch.target}</td>
                                                        <td>
                                                            <button on:click=move |_| send_test(test_id.clone())>
                                                                "Send test"
                                                            </button>
                                                            <button on:click=delete_channel>"Delete"</button>
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </TableShell>
                                }
                                    .into_any()
                            }
                        }}

                        <form on:submit=create_channel class="channel-form">
                            <input
                                type="text"
                                placeholder="Notification name"
                                prop:value=ch_name
                                on:input=move |ev| ch_name.set(event_target_value(&ev))
                                required
                            />
                            <select prop:value=ch_kind on:change=move |ev| ch_kind.set(event_target_value(&ev))>
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
                            <button type="submit">"Add Notification"</button>
                        </form>
                        {move || channel_error.get().map(|e| view! { <div class="error">{e}</div> })}
                    }
                        .into_any()
                }
            }}
        </div>
    }
}
