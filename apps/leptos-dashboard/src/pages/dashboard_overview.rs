//! Flagship dashboard overview backed exclusively by the aggregated backend contract.

use crate::api::monitoring::{self, DashboardOverview, DashboardTrendPoint, MonitorOverviewItem};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

const RECENT_MONITORS_LIMIT: usize = 6;
const TREND_HEIGHT: f64 = 120.0;

fn user_facing_error(message: &str) -> String {
    if message.starts_with("Network error:") {
        "Unable to reach PingVertex. Check your connection and try again.".to_string()
    } else if message.starts_with("Server error") {
        "PingVertex could not load the dashboard right now. Please try again.".to_string()
    } else {
        message.to_string()
    }
}

fn format_number(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn format_timestamp(value: &str) -> String {
    value
        .strip_suffix('Z')
        .unwrap_or(value)
        .replace('T', " ")
        .split('.')
        .next()
        .unwrap_or(value)
        .to_string()
}

fn trend_points(
    trend: &[DashboardTrendPoint],
    value: impl Fn(&DashboardTrendPoint) -> Option<f64>,
) -> Option<String> {
    let values: Vec<f64> = trend.iter().filter_map(value).collect();
    if values.len() < 2 {
        return None;
    }

    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(0.000_001);
    let width = 720.0;
    let step = width / (values.len() - 1) as f64;

    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f64 * step;
            let y = 8.0 + ((max - value) / range) * (TREND_HEIGHT - 16.0);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");

    Some(points)
}

#[component]
fn TrendChart(
    #[prop(into)] title: String,
    #[prop(into)] value: String,
    #[prop(into)] description: String,
    trend: Vec<DashboardTrendPoint>,
    #[prop(into)] metric: String,
    #[prop(into)] unit: String,
) -> impl IntoView {
    let points = if metric == "uptime" {
        trend_points(&trend, |point| point.uptime_percentage)
    } else if metric == "latency" {
        trend_points(&trend, |point| point.avg_latency_ms)
    } else {
        trend_points(&trend, |point| Some(point.incident_count as f64))
    };

    let empty = points.is_none();
    let aria = if empty {
        format!("{title}: insufficient data for a trend")
    } else {
        format!("{title}: last 24 hours")
    };

    view! {
        <section class="dashboard-widget dashboard-trend-card" aria-label=aria>
            <div class="widget-heading">
                <div>
                    <h2>{title}</h2>
                    <p class="widget-description">{description}</p>
                </div>
                <strong class="trend-value">{value} {unit}</strong>
            </div>
            {if let Some(points) = points {
                view! {
                    <div class="trend-chart-wrap">
                        <svg class="trend-chart" viewBox="0 0 720 120" role="img" aria-label="24 hour trend">
                            <polyline
                                class="trend-line"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="3"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                points=points
                            />
                        </svg>
                    </div>
                    <div class="trend-axis" aria-hidden="true">
                        <span>"24h ago"</span>
                        <span>"Now"</span>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="trend-empty">
                        <span>"Not enough telemetry yet."</span>
                        <small>"A trend appears after at least two data points are available."</small>
                    </div>
                }.into_any()
            }}
        </section>
    }
}

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let overview = RwSignal::new(None::<DashboardOverview>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let last_updated = RwSignal::new(None::<String>);

    let load_data = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        error.set(None);

        spawn_local(async move {
            match monitoring::fetch_overview().await {
                Ok(data) => {
                    overview.set(Some(data));
                    let updated = js_sys::Date::new_0()
                        .to_iso_string()
                        .as_string()
                        .unwrap_or_else(|| "just now".to_string());
                    last_updated.set(Some(updated));
                }
                Err(e) => error.set(Some(user_facing_error(&e))),
            }
            loading.set(false);
        });
    };

    // Initial load must run even though the refresh guard is enabled.
    spawn_local(async move {
        match monitoring::fetch_overview().await {
            Ok(data) => {
                overview.set(Some(data));
                let updated = js_sys::Date::new_0()
                    .to_iso_string()
                    .as_string()
                    .unwrap_or_else(|| "just now".to_string());
                last_updated.set(Some(updated));
            }
            Err(e) => error.set(Some(user_facing_error(&e))),
        }
        loading.set(false);
    });

    let on_retry = Callback::new(move |_: ()| load_data());
    let refresh_click = move |_| load_data();

    view! {
        <div class="dashboard-overview">
            <header class="dashboard-page-header">
                <div>
                    <p class="eyebrow">"SYSTEM OVERVIEW"</p>
                    <h1>"Overview"</h1>
                    <p class="page-subtitle">
                        {move || match last_updated.get() {
                            Some(value) => format!("Last updated {}", format_timestamp(&value)),
                            None => "Waiting for live telemetry".to_string(),
                        }}
                    </p>
                </div>
                <button
                    class="btn"
                    on:click=refresh_click
                    disabled=move || loading.get()
                    aria-label="Refresh dashboard"
                >
                    {move || if loading.get() { "Refreshing…" } else { "Refresh" }}
                </button>
            </header>

            {move || {
                if loading.get() && overview.get().is_none() {
                    view! { <LoadingSpinner text="Loading live dashboard data..."/> }.into_any()
                } else if error.get().is_some() && overview.get().is_none() {
                    let err = error.get().unwrap_or_else(|| "Unable to load dashboard.".to_string());
                    view! { <ErrorState title="Failed to load dashboard" description=err on_retry=on_retry/> }.into_any()
                } else if let Some(data) = overview.get() {
                    let summary = data.monitor_summary;
                    let performance = data.performance;
                    let recent_monitors: Vec<MonitorOverviewItem> =
                        data.monitors.into_iter().take(RECENT_MONITORS_LIMIT).collect();
                    let incidents = data.incident_summary.recent;
                    let open_incidents = data.incident_summary.open_count;

                    let uptime = performance
                        .uptime_percentage
                        .map(|v| format!("{}%", format_number(v)))
                        .unwrap_or_else(|| "—".to_string());
                    let latency = performance
                        .avg_latency_ms
                        .map(|v| format!("{} ms", format_number(v)))
                        .unwrap_or_else(|| "—".to_string());
                    let incident_total_24h: u32 = performance
                        .trend
                        .iter()
                        .map(|point| point.incident_count)
                        .sum();

                    view! {
                        <div class="dashboard-content">
                            {move || error.get().map(|message| view! {
                                <div class="dashboard-refresh-error" role="alert">
                                    <span>{message}</span>
                                    <button class="btn" on:click=move |_| load_data() disabled=move || loading.get()>
                                        "Retry"
                                    </button>
                                </div>
                            })}
                            <section class="metric-strip" aria-label="Monitor health summary">
                                <div class="card metric-card metric-primary">
                                    <span class="card-label">"Monitors"</span>
                                    <span class="card-value">{summary.total}</span>
                                    <span class="card-hint">"configured"</span>
                                </div>
                                <div class="card metric-card">
                                    <span class="card-label">"Up"</span>
                                    <span class="card-value up">{summary.up}</span>
                                    <span class="card-hint">"latest check"</span>
                                </div>
                                <div class="card metric-card">
                                    <span class="card-label">"Down"</span>
                                    <span class="card-value down">{summary.down}</span>
                                    <span class="card-hint">"latest check"</span>
                                </div>
                                <div class="card metric-card">
                                    <span class="card-label">"Disabled"</span>
                                    <span class="card-value">{summary.disabled}</span>
                                    <span class="card-hint">"not monitored"</span>
                                </div>
                                <div class="card metric-card">
                                    <span class="card-label">"Active incidents"</span>
                                    <span class="card-value">{open_incidents}</span>
                                    <span class="card-hint">"currently open"</span>
                                </div>
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading">
                                    <div>
                                        <p class="eyebrow">"PERFORMANCE · 24H"</p>
                                        <h2>"Performance"</h2>
                                    </div>
                                </div>
                                <div class="dashboard-trend-grid">
                                    <TrendChart
                                        title="Uptime".to_string()
                                        value=uptime
                                        description="Observed successful checks".to_string()
                                        trend=performance.trend.clone()
                                        metric="uptime".to_string()
                                        unit="".to_string()
                                    />
                                    <TrendChart
                                        title="Response time".to_string()
                                        value=latency
                                        description="Observed check latency".to_string()
                                        trend=performance.trend.clone()
                                        metric="latency".to_string()
                                        unit="".to_string()
                                    />
                                    <TrendChart
                                        title="Incident activity".to_string()
                                        value=incident_total_24h.to_string()
                                        description="Incidents started in the last 24 hours".to_string()
                                        trend=performance.trend.clone()
                                        metric="incidents".to_string()
                                        unit="".to_string()
                                    />
                                </div>
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading">
                                    <div>
                                        <p class="eyebrow">"MONITORS"</p>
                                        <h2>"Monitor health"</h2>
                                    </div>
                                    <A href="/monitors">"View all"</A>
                                </div>

                                {if recent_monitors.is_empty() {
                                    view! {
                                        <div class="dashboard-empty">
                                            <h3>"No monitors yet"</h3>
                                            <p>"Create your first monitor to start collecting live health data."</p>
                                            <A href="/monitors/new" attr:class="btn btn-primary">"Create monitor"</A>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="monitor-table-wrap">
                                            <table class="dashboard-monitor-table">
                                                <thead>
                                                    <tr>
                                                        <th scope="col">"Monitor"</th>
                                                        <th scope="col">"Status"</th>
                                                        <th scope="col">"Latency"</th>
                                                        <th scope="col">"Last check"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {recent_monitors.into_iter().map(|monitor| {
                                                        let id = monitor.id.clone();
                                                        let name = monitor.name.clone();
                                                        let status = monitor.status.clone().unwrap_or_else(|| {
                                                            if monitor.enabled { "unknown".to_string() } else { "disabled".to_string() }
                                                        });
                                                        let latency = monitor.latency_ms
                                                            .map(|v| format!("{v} ms"))
                                                            .unwrap_or_else(|| "—".to_string());
                                                        let checked_at = monitor.checked_at
                                                            .as_deref()
                                                            .map(format_timestamp)
                                                            .unwrap_or_else(|| "No check yet".to_string());

                                                        view! {
                                                            <tr>
                                                                <td>
                                                                    <A href=format!("/monitors/{id}") attr:class="monitor-link">
                                                                        {name}
                                                                    </A>
                                                                </td>
                                                                <td><StatusBadge status=status/></td>
                                                                <td class="data-cell">{latency}</td>
                                                                <td class="data-cell muted-cell">{checked_at}</td>
                                                            </tr>
                                                        }
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        </div>
                                    }.into_any()
                                }}
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading">
                                    <div>
                                        <p class="eyebrow">"INCIDENTS"</p>
                                        <h2>"Incident activity"</h2>
                                    </div>
                                    <A href="/monitors">"View monitors"</A>
                                </div>

                                {if incidents.is_empty() {
                                    view! {
                                        <div class="dashboard-empty compact">
                                            <h3>"No incident activity"</h3>
                                            <p>"No incidents have been recorded in the available history."</p>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <ul class="incident-list">
                                            {incidents.into_iter().map(|incident| {
                                                let monitor_id = incident.monitor_id.clone();
                                                let started = format_timestamp(&incident.started_at);
                                                view! {
                                                    <li class="incident-item">
                                                        <A href=format!("/monitors/{monitor_id}") attr:class="incident-link">
                                                            <span class="incident-marker" aria-hidden="true"></span>
                                                            <span class="incident-copy">
                                                                <strong>{format!("Incident {}", &incident.id[..8.min(incident.id.len())])}</strong>
                                                                <small>{started}</small>
                                                            </span>
                                                            <span class="incident-arrow" aria-hidden="true">"→"</span>
                                                        </A>
                                                    </li>
                                                }
                                            }).collect_view()}
                                        </ul>
                                    }.into_any()
                                }}
                            </section>

                            <div class="dashboard-data-note">
                                "Dashboard values are derived from live monitor checks and incident records. No client-side mock or placeholder telemetry is used."
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <p class="widget-empty">"No dashboard data available."</p> }.into_any()
                }
            }}
        </div>
    }
}
