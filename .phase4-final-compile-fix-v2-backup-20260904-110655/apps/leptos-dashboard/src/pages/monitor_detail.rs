use crate::api::monitoring::{
    self, ChannelPayload, Check, EscalationPolicy, Incident, MaintenanceWindow, Monitor,
    NotificationChannel,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::empty_state::EmptyState;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::PageHeader;
use crate::components::status_badge::StatusBadge;
use crate::components::table_shell::TableShell;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

fn status_from_checks(monitor: &Monitor, checks: &[Check]) -> String {
    if !monitor.enabled {
        return "disabled".into();
    }
    checks
        .first()
        .map(|check| check.status.clone())
        .unwrap_or_else(|| "unknown".into())
}

fn uptime_from_checks(checks: &[Check]) -> Option<f64> {
    if checks.is_empty() {
        return None;
    }
    let up = checks
        .iter()
        .filter(|check| check.status.eq_ignore_ascii_case("up"))
        .count();
    Some((up as f64 / checks.len() as f64) * 100.0)
}

fn average_latency(checks: &[Check]) -> Option<f64> {
    if checks.is_empty() {
        None
    } else {
        Some(checks.iter().map(|check| check.latency_ms).sum::<u128>() as f64 / checks.len() as f64)
    }
}

fn incident_status(incident: &Incident) -> &'static str {
    if incident.resolved_at.is_some() {
        "resolved"
    } else if incident.acknowledged_at.is_some() {
        "acknowledged"
    } else {
        "open"
    }
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "http" | "Http" => "HTTP",
        "tcp" | "Tcp" => "TCP",
        "dns" | "Dns" => "DNS",
        "heartbeat" | "Heartbeat" => "Heartbeat",
        "flow" | "Flow" => "API flow",
        _ => kind,
    }
}

#[component]
pub fn MonitorDetailPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let monitor_id = move || params.get().get("id").unwrap_or_default();

    let monitor = RwSignal::new(None::<Monitor>);
    let checks = RwSignal::new(Vec::<Check>::new());
    let incidents = RwSignal::new(Vec::<Incident>::new());
    let channels = RwSignal::new(Vec::<NotificationChannel>::new());
    let maintenance_windows = RwSignal::new(Vec::<MaintenanceWindow>::new());
    let escalation_policies = RwSignal::new(Vec::<EscalationPolicy>::new());

    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let action_busy = RwSignal::new(false);
    let show_delete_confirm = RwSignal::new(false);
    let channel_to_delete = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    Effect::new(move |_| {
        let query = query.get();
        if query.get("saved").is_some() {
            notice.set(Some("Monitor saved successfully.".into()));
        } else if query.get("created").is_some() {
            notice.set(Some("Monitor created successfully.".into()));
        }
    });

    let load_data = move || {
        let id = monitor_id();
        if id.is_empty() {
            error.set(Some("Invalid monitor URL.".into()));
            loading.set(false);
            return;
        }

        loading.set(true);
        error.set(None);
        spawn_local(async move {
            // Keep the existing per-resource endpoints. They are already tenant
            // scoped server-side; this page only composes them for the detail view.
            let monitor_result = monitoring::get_monitor(&id).await;
            match monitor_result {
                Ok(value) => monitor.set(Some(value)),
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                    return;
                }
            }

            match monitoring::fetch_checks(&id).await {
                Ok(value) => checks.set(value),
                Err(e) => error.set(Some(format!("Checks: {e}"))),
            }
            match monitoring::fetch_incidents(&id).await {
                Ok(value) => incidents.set(value),
                Err(e) => error.set(Some(format!("Incidents: {e}"))),
            }
            match monitoring::list_notification_channels(&id).await {
                Ok(value) => channels.set(value),
                Err(e) => error.set(Some(format!("Notifications: {e}"))),
            }
            match monitoring::list_maintenance_windows(&id).await {
                Ok(value) => maintenance_windows.set(value),
                Err(e) => error.set(Some(format!("Maintenance: {e}"))),
            }
            match monitoring::list_escalation_policies().await {
                Ok(value) => escalation_policies.set(value),
                Err(e) => error.set(Some(format!("Escalation policies: {e}"))),
            }

            loading.set(false);
        });
    };

    Effect::new(move |_| load_data());

    let navigate_for_delete = navigate.clone();
    let confirm_delete_cb = Callback::new(move |_: ()| {
        if action_busy.get() {
            return;
        }
        let id = monitor_id();
        let navigate = navigate_for_delete.clone();
        action_busy.set(true);
        show_delete_confirm.set(false);
        spawn_local(async move {
            match monitoring::delete_monitor(&id).await {
                Ok(()) => navigate("/monitors", Default::default()),
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    });

    let cancel_delete_cb = Callback::new(move |_: ()| {
        show_delete_confirm.set(false);
    });

    let toggle_enabled = move |_| {
        if action_busy.get() {
            return;
        }
        let id = monitor_id();
        let current = monitor.get().map(|m| m.enabled).unwrap_or(false);
        action_busy.set(true);
        spawn_local(async move {
            match monitoring::set_monitor_enabled(&id, !current).await {
                Ok(()) => {
                    notice.set(Some(if current {
                        "Monitor disabled.".into()
                    } else {
                        "Monitor enabled.".into()
                    }));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    };

    let acknowledge = move |incident_id: String| {
        if action_busy.get() {
            return;
        }
        action_busy.set(true);
        spawn_local(async move {
            match monitoring::acknowledge_incident(&incident_id).await {
                Ok(()) => {
                    notice.set(Some("Incident acknowledged.".into()));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    };

    let send_test = move |channel_id: String| {
        if action_busy.get() {
            return;
        }
        action_busy.set(true);
        spawn_local(async move {
            match monitoring::test_notification_channel(&channel_id).await {
                Ok(()) => notice.set(Some("Test notification sent.".into())),
                Err(e) => error.set(Some(e)),
            }
            action_busy.set(false);
        });
    };

    let ch_kind = RwSignal::new(String::new());
    let ch_target = RwSignal::new(String::new());
    let ch_chat_id = RwSignal::new(String::new());
    let channel_error = RwSignal::new(None::<String>);
    let channel_secret = RwSignal::new(None::<String>);

    let create_channel = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        channel_error.set(None);
        let id = monitor_id();
        let kind = ch_kind.get().trim().to_string();
        let target = ch_target.get().trim().to_string();
        let chat_id = ch_chat_id.get().trim().to_string();

        if kind.is_empty() {
            channel_error.set(Some("Choose a notification type.".into()));
            return;
        }
        if target.is_empty() {
            channel_error.set(Some("Notification target is required.".into()));
            return;
        }
        if kind == "telegram" && chat_id.is_empty() {
            channel_error.set(Some("Telegram chat ID is required.".into()));
            return;
        }
        if action_busy.get() {
            return;
        }

        action_busy.set(true);
        let payload = ChannelPayload {
            kind,
            target,
            chat_id: if chat_id.is_empty() {
                None
            } else {
                Some(chat_id)
            },
        };
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &payload).await {
                Ok(created) => {
                    channel_secret.set(created.signing_secret);
                    ch_kind.set(String::new());
                    ch_target.set(String::new());
                    ch_chat_id.set(String::new());
                    notice.set(Some("Notification channel created.".into()));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    channel_error.set(Some(e));
                }
            }
        });
    };

    let confirm_channel_delete = Callback::new(move |_: ()| {
        let channel_id = match channel_to_delete.get() {
            Some(id) => id,
            None => return,
        };
        if action_busy.get() {
            return;
        }
        action_busy.set(true);
        channel_to_delete.set(None);
        spawn_local(async move {
            match monitoring::delete_notification_channel(&channel_id).await {
                Ok(()) => {
                    notice.set(Some("Notification channel deleted.".into()));
                    action_busy.set(false);
                    load_data();
                }
                Err(e) => {
                    action_busy.set(false);
                    error.set(Some(e));
                }
            }
        });
    });

    let cancel_channel_delete = Callback::new(move |_: ()| {
        channel_to_delete.set(None);
    });

    view! {
        <div class="monitor-detail">
            {move || {
                if loading.get() && monitor.get().is_none() {
                    view! {
                        <div class="data-section">
                            <div class="loading">"Loading monitor…"</div>
                        </div>
                    }.into_any()
                } else if monitor.get().is_none() {
                    view! {
                        <div class="error-state">
                            <h2>"Monitor unavailable"</h2>
                            <p>{move || error.get().unwrap_or_else(|| "The monitor could not be loaded.".into())}</p>
                            <button class="btn" on:click=move |_| load_data()>"Retry"</button>
                        </div>
                    }.into_any()
                } else {
                    let m = monitor.get().unwrap_or_default();
                    let status = status_from_checks(&m, &checks.get());
                    let uptime = uptime_from_checks(&checks.get())
                        .map(|v| format!("{v:.2}%"))
                        .unwrap_or_else(|| "—".into());
                    let avg_latency = average_latency(&checks.get())
                        .map(|v| format!("{v:.0} ms"))
                        .unwrap_or_else(|| "—".into());
                    let latest = checks.get().first().cloned();
                    let ssl_status = if !m.ssl_check_enabled {
                        "Disabled".to_string()
                    } else if let Some(days) = latest.as_ref().and_then(|c| c.cert_days_remaining) {
                        if days < 0 {
                            format!("Expired ({days}d)")
                        } else if days <= m.ssl_expiry_warning_days as i64 {
                            format!("Expires in {days}d")
                        } else {
                            format!("Valid ({days}d)")
                        }
                    } else {
                        "No certificate data".into()
                    };
                    let max_latency = checks
                        .get()
                        .iter()
                        .map(|c| c.latency_ms)
                        .max()
                        .unwrap_or(1);

                    let navigate_edit = navigate.clone();
                    let monitor_id_for_edit = monitor_id();

                    view! {
                        <PageHeader title=m.name.clone()>
                            <button
                                class="btn"
                                disabled=move || action_busy.get()
                                on:click=move |_| navigate_edit(&format!("/monitors/{monitor_id_for_edit}/edit"), Default::default())
                            >"Edit"</button>
                            <button
                                class="btn"
                                disabled=move || action_busy.get()
                                on:click=toggle_enabled
                            >{if m.enabled { "Disable" } else { "Enable" }}</button>
                            <button
                                class="btn btn-danger"
                                disabled=move || action_busy.get()
                                on:click=move |_| show_delete_confirm.set(true)
                            >"Delete"</button>
                        </PageHeader>

                        {move || error.get().map(|e| view! {
                            <div class="alert alert-error" role="alert">{e}</div>
                        })}
                        {move || notice.get().map(|n| view! {
                            <div class="alert alert-success" role="status" aria-live="polite">{n}</div>
                        })}

                        {move || if show_delete_confirm.get() {
                            view! {
                                <ConfirmDialog
                                    title="Delete monitor?"
                                    body="This permanently deletes the monitor and its recorded history. This action cannot be undone."
                                    on_confirm=confirm_delete_cb
                                    on_cancel=cancel_delete_cb
                                />
                            }.into_any()
                        } else {
                            ().into_any()
                        }}

                        {move || if channel_to_delete.get().is_some() {
                            view! {
                                <ConfirmDialog
                                    title="Delete notification channel?"
                                    body="This removes the channel from this monitor. You can create it again later."
                                    on_confirm=confirm_channel_delete
                                    on_cancel=cancel_channel_delete
                                />
                            }.into_any()
                        } else {
                            ().into_any()
                        }}

                        <section class="monitor-identity data-section">
                            <div>
                                <span class="eyebrow">"Monitor"</span>
                                <h2>{m.name.clone()}</h2>
                                <p class="muted">{format!("{} · {} · {}", kind_label(&m.kind), m.method, m.url.clone().unwrap_or_else(|| "No target".into()))}</p>
                            </div>
                            <div class="monitor-status">
                                <StatusBadge status=status.clone()/>
                            </div>
                        </section>

                        <section class="metric-strip monitor-health-metrics">
                            <MetricCard label="Current status" value=status/>
                            <MetricCard label="Uptime (last 50 checks)" value=uptime/>
                            <MetricCard label="Avg response" value=avg_latency/>
                            <MetricCard label="Check frequency" value=format!("{}s", m.interval_seconds)/>
                            <MetricCard label="SSL" value=ssl_status/>
                        </section>

                        <section class="data-section">
                            <div class="section-heading">
                                <div>
                                    <h2>"Performance"</h2>
                                    <p class="muted">"Recent response-time trend · newest check first"</p>
                                </div>
                            </div>
                            {move || if checks.get().is_empty() {
                                view! {
                                    <EmptyState title="No performance data" description="Performance data will appear after the monitor runs."/>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="performance-bars" aria-label="Recent response time chart">
                                        {checks.get().iter().take(20).enumerate().map(|(index, check)| {
                                            let latency = check.latency_ms;
                                            let width = if latency == 0 { 2 } else {
                                                ((latency as f64 / max_latency as f64) * 100.0).max(4.0) as u32
                                            };
                                            let label = format!("{} · {} ms", check.checked_at, latency);
                                            let status = check.status.clone();
                                            view! {
                                                <div class="performance-row" title=label>
                                                    <span class="performance-index">{format!("{}", index + 1)}</span>
                                                    <div class="performance-track">
                                                        <div
                                                            class=format!("performance-fill performance-{}", status.to_lowercase())
                                                            style=format!("width:{width}%")
                                                        ></div>
                                                    </div>
                                                    <span class="performance-value">{format!("{latency} ms")}</span>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Recent checks"</h2><span class="muted">{format!("{} loaded", checks.get().len())}</span></div>
                            {move || if checks.get().is_empty() {
                                view! {
                                    <EmptyState title="No checks recorded" description="Checks will appear here once this monitor runs."/>
                                }.into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Checked at".into(),
                                        "Status".into(),
                                        "Response".into(),
                                        "Region".into(),
                                        "TLS".into(),
                                        "Error".into(),
                                    ]>
                                        {checks.get().into_iter().take(20).map(|check| view! {
                                            <tr>
                                                <td>{check.checked_at}</td>
                                                <td><StatusBadge status=check.status.clone()/></td>
                                                <td>{format!("{} ms", check.latency_ms)}</td>
                                                <td>{check.region}</td>
                                                <td>{check.cert_days_remaining.map(|v| format!("{v}d")).unwrap_or_else(|| "—".into())}</td>
                                                <td>{check.error_message.unwrap_or_else(|| "—".into())}</td>
                                            </tr>
                                        }).collect_view()}
                                    </TableShell>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Incidents"</h2><span class="muted">{format!("{}", incidents.get().len())}</span></div>
                            {move || if incidents.get().is_empty() {
                                view! {
                                    <EmptyState title="No incidents" description="This monitor has no recorded incidents."/>
                                }.into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "First seen".into(),
                                        "Status".into(),
                                        "Duration".into(),
                                        "Last seen".into(),
                                        "Maintenance".into(),
                                        "Actions".into(),
                                    ]>
                                        {incidents.get().into_iter().map(|incident| {
                                            let id = incident.id.clone();
                                            let status = incident_status(&incident);
                                            let can_ack = status == "open";
                                            view! {
                                                <tr>
                                                    <td>{incident.first_seen}</td>
                                                    <td><StatusBadge status=status.to_string()/></td>
                                                    <td>{format!("{} ms", incident.duration_ms)}</td>
                                                    <td>{incident.last_seen_at.unwrap_or_else(|| "—".into())}</td>
                                                    <td>{if incident.occurred_during_maintenance { "Yes" } else { "No" }}</td>
                                                    <td>
                                                        {if can_ack {
                                                            view! {
                                                                <button
                                                                    class="btn"
                                                                    disabled=move || action_busy.get()
                                                                    on:click=move |_| acknowledge(id.clone())
                                                                >"Acknowledge"</button>
                                                            }.into_any()
                                                        } else {
                                                            ().into_any()
                                                        }}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </TableShell>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Notifications"</h2></div>
                            {move || if channels.get().is_empty() {
                                view! {
                                    <EmptyState title="No notifications configured" description="Add a channel below to receive monitor alerts."/>
                                }.into_any()
                            } else {
                                view! {
                                    <TableShell headers=vec![
                                        "Type".into(),
                                        "Target".into(),
                                        "Enabled".into(),
                                        "Secret".into(),
                                        "Actions".into(),
                                    ]>
                                        {channels.get().into_iter().map(|channel| {
                                            let id = channel.id.clone();
                                            let test_id = id.clone();
                                            view! {
                                                <tr>
                                                    <td>{channel.kind.clone()}</td>
                                                    <td class="mono">{channel.target.clone()}</td>
                                                    <td>{if channel.enabled { "Enabled" } else { "Disabled" }}</td>
                                                    <td>{if channel.has_signing_secret { "Configured" } else { "—" }}</td>
                                                    <td>
                                                        <button class="btn" disabled=move || action_busy.get() on:click=move |_| send_test(test_id.clone())>"Send test"</button>
                                                        <button class="btn btn-danger" disabled=move || action_busy.get() on:click=move |_| channel_to_delete.set(Some(id.clone()))>"Delete"</button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </TableShell>
                                }.into_any()
                            }}

                            <form on:submit=create_channel class="channel-form monitor-subform">
                                <select
                                    prop:value=ch_kind
                                    on:change=move |ev| ch_kind.set(event_target_value(&ev))
                                    required
                                    aria-label="Notification type"
                                >
                                    <option value="">"Notification type"</option>
                                    <option value="email">"Email"</option>
                                    <option value="webhook">"Webhook"</option>
                                    <option value="slack">"Slack"</option>
                                    <option value="discord">"Discord"</option>
                                    <option value="telegram">"Telegram"</option>
                                    <option value="sms">"SMS"</option>
                                </select>
                                <input
                                    type="text"
                                    placeholder="Target (email, webhook URL, bot token, etc.)"
                                    prop:value=ch_target
                                    on:input=move |ev| ch_target.set(event_target_value(&ev))
                                    maxlength="2048"
                                    required
                                />
                                {move || if ch_kind.get() == "telegram" {
                                    view! {
                                        <input
                                            type="text"
                                            placeholder="Telegram chat ID"
                                            prop:value=ch_chat_id
                                            on:input=move |ev| ch_chat_id.set(event_target_value(&ev))
                                            maxlength="128"
                                            required
                                        />
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}
                                <button class="btn btn-primary" type="submit" disabled=move || action_busy.get()>"Add channel"</button>
                            </form>
                            {move || channel_error.get().map(|e| view! {
                                <div class="form-error-message" role="alert">{e}</div>
                            })}
                            {move || channel_secret.get().map(|secret| view! {
                                <div class="alert alert-warning" role="alert">
                                    <strong>"Save this webhook signing secret now. "</strong>
                                    "It will not be shown again: "
                                    <code>{secret}</code>
                                </div>
                            })}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Maintenance"</h2></div>
                            {move || if maintenance_windows.get().is_empty() {
                                view! {
                                    <EmptyState title="No maintenance windows" description="No maintenance windows are configured for this monitor."/>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="list-stack">
                                        {maintenance_windows.get().into_iter().take(20).map(|window| view! {
                                            <article class="list-row">
                                                <div>
                                                    <strong>{window.reason}</strong>
                                                    <div class="muted">{format!("{} → {}", window.starts_at, window.ends_at)}</div>
                                                </div>
                                            </article>
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </section>

                        <section class="data-section">
                            <div class="section-heading"><h2>"Escalation"</h2></div>
                            {move || {
                                let id = monitor_id();
                                let policies: Vec<EscalationPolicy> = escalation_policies
                                    .get()
                                    .into_iter()
                                    .filter(|policy| policy.monitor_id.as_deref() == Some(id.as_str()))
                                    .collect();
                                if policies.is_empty() {
                                    view! {
                                        <EmptyState title="No monitor-specific policy" description="No escalation policy is attached directly to this monitor."/>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="list-stack">
                                            {policies.into_iter().map(|policy| view! {
                                                <article class="list-row">
                                                    <div>
                                                        <strong>{format!("Policy {}", policy.id)}</strong>
                                                        <div class="muted">{format!("{} step(s)", policy.steps.len())}</div>
                                                        <ol class="escalation-steps">
                                                            {policy.steps.into_iter().enumerate().map(|(index, step)| view! {
                                                                <li>{format!("Step {} · after {} min · channel {}", index + 1, step.delay_minutes, step.channel_id)}</li>
                                                            }).collect_view()}
                                                        </ol>
                                                    </div>
                                                </article>
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </section>
                    }.into_any()
                }
            }}
        </div>
    }
}
