//! Flagship dashboard overview backed exclusively by the aggregated backend contract.

use crate::api::monitoring::{self, DashboardOverview, DashboardTrendPoint, MonitorOverviewItem};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::polling;
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

struct TrendMarker {
    x: f64,
    y: f64,
    bucket: String,
    value: f64,
}

struct TrendSeries {
    segments: Vec<String>,
    markers: Vec<TrendMarker>,
}

fn trend_series(
    trend: &[DashboardTrendPoint],
    value: impl Fn(&DashboardTrendPoint) -> Option<f64>,
) -> Option<TrendSeries> {
    if trend.len() < 2 {
        return None;
    }

    let present: Vec<(usize, f64)> = trend
        .iter()
        .enumerate()
        .filter_map(|(index, point)| value(point).map(|v| (index, v)))
        .collect();

    if present.len() < 2 {
        return None;
    }

    let min = present
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::INFINITY, f64::min);
    let max = present
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(0.000_001);

    let width = 720.0;
    let slot_count = trend.len();
    let step = if slot_count > 1 {
        width / (slot_count - 1) as f64
    } else {
        0.0
    };

    let coord = |index: usize, v: f64| -> (f64, f64) {
        let x = index as f64 * step;
        let y = 8.0 + ((max - v) / range) * (TREND_HEIGHT - 16.0);
        (x, y)
    };

    let markers: Vec<TrendMarker> = present
        .iter()
        .map(|(index, v)| {
            let (x, y) = coord(*index, *v);
            TrendMarker {
                x,
                y,
                bucket: trend[*index].bucket.clone(),
                value: *v,
            }
        })
        .collect();

    let mut segments: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut prev_index: Option<usize> = None;

    for (index, v) in &present {
        let (x, y) = coord(*index, *v);
        let is_contiguous = prev_index.map(|prev| *index == prev + 1).unwrap_or(true);
        if !is_contiguous && !current.is_empty() {
            if current.len() >= 2 {
                segments.push(current.join(" "));
            }
            current = Vec::new();
        }
        current.push(format!("{x:.1},{y:.1}"));
        prev_index = Some(*index);
    }
    if current.len() >= 2 {
        segments.push(current.join(" "));
    }

    Some(TrendSeries { segments, markers })
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
    let series = if metric == "uptime" {
        trend_series(&trend, |point| point.uptime_percentage)
    } else if metric == "latency" {
        trend_series(&trend, |point| point.avg_latency_ms)
    } else {
        trend_series(&trend, |point| Some(point.incident_count as f64))
    };

    let empty = series.is_none();
    let aria = if empty {
        format!("{title}: insufficient data for a trend")
    } else {
        format!("{title}: last 24 hours")
    };

    let metric_for_format = metric.clone();
    let format_marker_value = move |v: f64| -> String {
        match metric_for_format.as_str() {
            "uptime" => format!("{v:.1}%"),
            "latency" => format!("{v:.0} ms"),
            _ => format!("{v:.0} incidents"),
        }
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
            {if let Some(series) = series {
                let segment_views = series.segments.iter().map(|points| {
                    view! {
                        <polyline
                            class="trend-line"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="3"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            points=points.clone()
                        />
                    }
                }).collect_view();

                let marker_views = series.markers.iter().map(|marker| {
                    let cx = format!("{:.1}", marker.x);
                    let cy = format!("{:.1}", marker.y);
                    let bucket_label = format_timestamp(&marker.bucket);
                    let value_label = format_marker_value(marker.value);
                    let point_label = format!("{bucket_label}: {value_label}");
                    view! {
                        <circle
                            class="trend-point"
                            cx=cx
                            cy=cy
                            r="4"
                            fill="currentColor"
                            tabindex="0"
                            role="img"
                            aria-label=point_label.clone()
                        >
                            <title>{point_label.clone()}</title>
                        </circle>
                    }
                }).collect_view();

                view! {
                    <div class="trend-chart-wrap">
                        <svg class="trend-chart" viewBox="0 0 720 120" role="img" aria-label="24 hour trend">
                            {segment_views}
                            {marker_views}
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
    let refreshing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let last_updated = RwSignal::new(None::<String>);
    let in_flight = RwSignal::new(false);

    let poll_data = move || {
        let overview = overview;
        let loading = loading;
        let refreshing = refreshing;
        let error = error;
        let last_updated = last_updated;
        let in_flight = in_flight;

        async move {
            if in_flight.get_untracked() {
                return Ok(());
            }
            in_flight.set(true);
            refreshing.set(true);

            let result = monitoring::fetch_overview().await;
            match result {
                Ok(data) => {
                    overview.set(Some(data));
                    let updated = js_sys::Date::new_0()
                        .to_iso_string()
                        .as_string()
                        .unwrap_or_else(|| "just now".to_string());
                    last_updated.set(Some(updated));
                    error.set(None);
                    loading.set(false);
                    refreshing.set(false);
                    in_flight.set(false);
                    Ok(())
                }
                Err(e) => {
                    error.set(Some(user_facing_error(&e)));
                    loading.set(false);
                    refreshing.set(false);
                    in_flight.set(false);
                    Err(())
                }
            }
        }
    };

    // First request is immediate; subsequent requests are visibility-aware,
    // single-flight and exponentially backed off after failures.
    spawn_local(async move {
        let _ = poll_data().await;
    });
    polling::start(poll_data);

    let on_retry = Callback::new(move |_: ()| {
        spawn_local(async move {
            // Manual retry deliberately bypasses the normal polling delay.
            if in_flight.get_untracked() {
                return;
            }
            in_flight.set(true);
            refreshing.set(true);
            error.set(None);
            match monitoring::fetch_overview().await {
                Ok(data) => {
                    overview.set(Some(data));
                    last_updated.set(Some(
                        js_sys::Date::new_0()
                            .to_iso_string()
                            .as_string()
                            .unwrap_or_else(|| "just now".to_string()),
                    ));
                    loading.set(false);
                }
                Err(e) => error.set(Some(user_facing_error(&e))),
            }
            refreshing.set(false);
            loading.set(false);
            in_flight.set(false);
        });
    });
    let refresh_click_cb = on_retry;
    let refresh_click = move |_| refresh_click_cb.run(());

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
                    disabled=move || refreshing.get()
                    aria-label="Refresh dashboard"
                >
                    {move || if refreshing.get() { "Refreshing…" } else { "Refresh" }}
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
                                    <button class="btn" on:click=move |_| on_retry.clone().run(()) disabled=move || refreshing.get()>
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
                                    <TrendChart title="Uptime".to_string() value=uptime description="Observed successful checks".to_string() trend=performance.trend.clone() metric="uptime".to_string() unit="".to_string()/>
                                    <TrendChart title="Response time".to_string() value=latency description="Observed check latency".to_string() trend=performance.trend.clone() metric="latency".to_string() unit="".to_string()/>
                                    <TrendChart title="Incident activity".to_string() value=incident_total_24h.to_string() description="Incidents started in the last 24 hours".to_string() trend=performance.trend.clone() metric="incidents".to_string() unit="".to_string()/>
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
                                                            .unwrap_or_else(|| "—".to_string());
                                                        view! {
                                                            <tr>
                                                                <td><A href=format!("/monitors/{id}") attr:class="monitor-link">{name}</A></td>
                                                                <td><StatusBadge status=status/></td>
                                                                <td class="mono">{latency}</td>
                                                                <td class="mono">{checked_at}</td>
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
                                        <h2>"Recent incidents"</h2>
                                    </div>
                                    <A href="/monitors">"View monitors"</A>
                                </div>
                                {if incidents.is_empty() {
                                    view! { <div class="dashboard-empty"><p>"No recent incidents."</p></div> }.into_any()
                                } else {
                                    view! {
                                        <div class="incident-list">
                                            {incidents.into_iter().map(|incident| {
                                                let href = format!("/monitors/{}", incident.monitor_id);
                                                let label = format_timestamp(&incident.started_at);
                                                view! {
                                                    <div class="incident-row">
                                                        <A href=href attr:class="incident-link">{incident.monitor_id}</A>
                                                        <span class="mono">{label}</span>
                                                        {if incident.acknowledged_at.is_some() {
                                                            view! { <StatusBadge status="acknowledged"/> }.into_any()
                                                        } else {
                                                            view! { <StatusBadge status="open"/> }.into_any()
                                                        }}
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }}
                            </section>
                        </div>
                    }.into_any()
                } else {
                    view! { <ErrorState title="Dashboard unavailable" description="No dashboard data is currently available." on_retry=on_retry/> }.into_any()
                }
            }}
        </div>
    }
}
