//! Flagship dashboard overview backed by tenant-scoped aggregate APIs.

use crate::api::monitoring::{self, DashboardOverview, DashboardTrendPoint, MonitorOverviewItem};
use crate::auth::context::{use_auth, AuthState};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::polling;
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

const RECENT_MONITORS_LIMIT: usize = 8;
const TREND_HEIGHT: f64 = 120.0;
const TREND_BOTTOM: f64 = 116.0;

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

fn hour_label(value: &str) -> String {
    let hour = value
        .split('T')
        .nth(1)
        .and_then(|part| part.get(0..2))
        .and_then(|h| h.parse::<u32>().ok())
        .unwrap_or(0);
    let suffix = if hour >= 12 { "PM" } else { "AM" };
    let display = match hour % 12 {
        0 => 12,
        value => value,
    };
    format!("{display}{suffix}")
}

fn relative_time(value: Option<&str>, now_ms: f64) -> String {
    let Some(value) = value else {
        return "No check yet".to_string();
    };
    let parsed = js_sys::Date::parse(value);
    if !parsed.is_finite() {
        return format_timestamp(value);
    }
    let seconds = ((now_ms - parsed) / 1000.0).round() as i64;
    if seconds < 0 {
        return "in a moment".to_string();
    }
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

fn delta_percent(first: Option<f64>, last: Option<f64>) -> Option<f64> {
    let (first, last) = (first?, last?);
    if first.abs() < f64::EPSILON {
        return None;
    }
    Some(((last - first) / first.abs()) * 100.0)
}

fn trend_delta(trend: &[DashboardTrendPoint], metric: &str) -> Option<f64> {
    let values: Vec<f64> = trend
        .iter()
        .filter_map(|point| match metric {
            "uptime" => point.uptime_percentage,
            "latency" => point.avg_latency_ms,
            _ => Some(point.incident_count as f64),
        })
        .collect();
    delta_percent(values.first().copied(), values.last().copied())
}

fn monitor_type(kind: &str, url: &str) -> (&'static str, &'static str) {
    match kind.to_ascii_lowercase().as_str() {
        "tcp" => ("tcp", "TCP"),
        "dns" => ("dns", "DNS"),
        "heartbeat" => ("heartbeat", "Heartbeat"),
        "flow" => ("flow", "API flow"),
        "http" if url.starts_with("https://") => ("https", "HTTPS"),
        _ => ("http", "HTTP"),
    }
}

fn monitor_status(monitor: &MonitorOverviewItem) -> String {
    if !monitor.enabled {
        "disabled".to_string()
    } else {
        monitor
            .status
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
            .to_ascii_lowercase()
    }
}

struct TrendPoint {
    x: f64,
    y: f64,
}

struct TrendSeries {
    line_paths: Vec<String>,
    area_paths: Vec<String>,
    markers: Vec<(TrendPoint, String, f64)>,
}

fn smooth_path(points: &[TrendPoint]) -> String {
    if points.len() < 2 {
        return String::new();
    }

    let mut path = format!("M {:.1},{:.1}", points[0].x, points[0].y);
    for i in 0..points.len() - 1 {
        let p0 = if i == 0 { &points[i] } else { &points[i - 1] };
        let p1 = &points[i];
        let p2 = &points[i + 1];
        let p3 = if i + 2 < points.len() {
            &points[i + 2]
        } else {
            p2
        };
        let c1x = p1.x + (p2.x - p0.x) / 6.0;
        let c1y = p1.y + (p2.y - p0.y) / 6.0;
        let c2x = p2.x - (p3.x - p1.x) / 6.0;
        let c2y = p2.y - (p3.y - p1.y) / 6.0;
        path.push_str(&format!(
            " C {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            c1x, c1y, c2x, c2y, p2.x, p2.y
        ));
    }
    path
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
    let step = width / (trend.len().saturating_sub(1).max(1) as f64);

    let point_for = |index: usize, v: f64| TrendPoint {
        x: index as f64 * step,
        y: 8.0 + ((max - v) / range) * (TREND_HEIGHT - 16.0),
    };

    let mut line_paths = Vec::new();
    let mut area_paths = Vec::new();
    let mut markers = Vec::new();
    let mut current: Vec<TrendPoint> = Vec::new();
    let mut start_index = 0usize;
    let mut previous_index: Option<usize> = None;

    for (index, value) in present {
        if previous_index
            .map(|previous| index != previous + 1)
            .unwrap_or(false)
        {
            if current.len() >= 2 {
                let line = smooth_path(&current);
                let area = format!(
                    "{line} L {:.1},{TREND_BOTTOM:.1} L {:.1},{TREND_BOTTOM:.1} Z",
                    current.last().unwrap().x,
                    current.first().unwrap().x
                );
                line_paths.push(line);
                area_paths.push(area);
            }
            current.clear();
            start_index = index;
        }

        let point = point_for(index, value);
        markers.push((point_for(index, value), trend[index].bucket.clone(), value));
        current.push(point);
        previous_index = Some(index);
    }

    let _ = start_index;
    if current.len() >= 2 {
        let line = smooth_path(&current);
        let area = format!(
            "{line} L {:.1},{TREND_BOTTOM:.1} L {:.1},{TREND_BOTTOM:.1} Z",
            current.last().unwrap().x,
            current.first().unwrap().x
        );
        line_paths.push(line);
        area_paths.push(area);
    }

    Some(TrendSeries {
        line_paths,
        area_paths,
        markers,
    })
}

#[component]
fn TrendIndicator(
    #[prop(optional)] delta: Option<f64>,
    #[prop(optional)] inverse: bool,
) -> impl IntoView {
    let Some(delta) = delta else {
        return view! { <span class="trend-indicator trend-neutral">"—"</span> }.into_any();
    };
    let positive = if inverse { delta < 0.0 } else { delta > 0.0 };
    let arrow = if delta.abs() < 0.05 {
        "→"
    } else if positive {
        "↑"
    } else {
        "↓"
    };
    let class = if delta.abs() < 0.05 {
        "trend-indicator trend-neutral"
    } else if positive {
        "trend-indicator trend-positive"
    } else {
        "trend-indicator trend-negative"
    };
    view! { <span class=class>{format!("{arrow} {:.1}%", delta.abs())}</span> }.into_any()
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
    let series = match metric.as_str() {
        "uptime" => trend_series(&trend, |point| point.uptime_percentage),
        "latency" => trend_series(&trend, |point| point.avg_latency_ms),
        _ => trend_series(&trend, |point| Some(point.incident_count as f64)),
    };
    let delta = trend_delta(&trend, &metric);
    let inverse = metric == "latency";
    let gradient_id = format!("trend-fill-{}", metric);

    let format_marker_value = |v: f64| match metric.as_str() {
        "uptime" => format!("{v:.2}%"),
        "latency" => format!("{v:.0} ms"),
        _ => format!("{v:.0} incidents"),
    };

    view! {
        <section class="dashboard-widget dashboard-trend-card" aria-label=format!("{}: last 24 hours", title.clone())>
            <div class="widget-heading">
                <div>
                    <div class="trend-title-row">
                        <h2>{title.clone()}</h2>
                        <TrendIndicator delta=delta.unwrap_or(0.0) inverse=inverse/>
                    </div>
                    <p class="widget-description">{description}</p>
                </div>
                <strong class="trend-value">{value} {unit}</strong>
            </div>
            {if let Some(series) = series {
                let area_views = series.area_paths.iter().map(|path| view! {
                    <path class="trend-area" d=path.clone() fill=format!("url(#{gradient_id})")/>
                }).collect_view();
                let line_views = series.line_paths.iter().map(|path| view! {
                    <path class="trend-line" d=path.clone() fill="none"/>
                }).collect_view();
                let marker_views = series.markers.iter().map(|(point, bucket, value)| {
                    let label = format!("{}: {}", format_timestamp(bucket), format_marker_value(*value));
                    view! {
                        <circle class="trend-point" cx=format!("{:.1}", point.x) cy=format!("{:.1}", point.y) r="3.5" tabindex="0" role="img" aria-label=label.clone()>
                            <title>{label.clone()}</title>
                        </circle>
                    }
                }).collect_view();
                let axis_labels = trend.iter().enumerate().filter_map(|(index, point)| {
                    if index % 4 == 0 { Some(hour_label(&point.bucket)) } else { None }
                }).collect::<Vec<_>>();
                view! {
                    <div class="trend-chart-wrap">
                        <svg class="trend-chart" viewBox="0 0 720 120" preserveAspectRatio="none" role="img" aria-label="24 hour performance trend">
                            <defs>
                                <linearGradient id=gradient_id x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="0%" stop-color="currentColor" stop-opacity="0.22"/>
                                    <stop offset="100%" stop-color="currentColor" stop-opacity="0"/>
                                </linearGradient>
                            </defs>
                            <line class="trend-grid" x1="0" y1="20" x2="720" y2="20"/>
                            <line class="trend-grid" x1="0" y1="60" x2="720" y2="60"/>
                            <line class="trend-grid" x1="0" y1="100" x2="720" y2="100"/>
                            {area_views}
                            {line_views}
                            {marker_views}
                        </svg>
                    </div>
                    <div class="trend-axis" aria-hidden="true">
                        {axis_labels.into_iter().map(|label| view! { <span>{label}</span> }).collect_view()}
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
fn StatSparkline(#[prop(optional)] values: Vec<f64>) -> impl IntoView {
    let width = 120.0;
    let height = 28.0;
    if values.len() < 2 {
        return view! { <div class="stat-sparkline stat-sparkline-empty" aria-hidden="true"></div> }.into_any();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(0.000_001);
    let step = width / (values.len() - 1) as f64;
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = index as f64 * step;
            let y = 4.0 + ((max - value) / range) * 20.0;
            TrendPoint { x, y }
        })
        .collect::<Vec<_>>();
    let line = smooth_path(&points);
    let area = format!("{line} L {width:.1},{height:.1} L 0,{height:.1} Z");
    view! {
        <svg class="stat-sparkline" viewBox="0 0 120 28" preserveAspectRatio="none" aria-hidden="true">
            <path class="spark-area" d=area/>
            <path class="spark-line" d=line fill="none"/>
        </svg>
    }.into_any()
}

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let auth = use_auth();
    let overview = RwSignal::new(None::<DashboardOverview>);
    let loading = RwSignal::new(true);
    let refreshing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let last_updated = RwSignal::new(None::<String>);
    let in_flight = RwSignal::new(false);
    let relative_now = RwSignal::new(js_sys::Date::new_0().get_time());
    let monitor_filter = RwSignal::new(String::from("all"));
    let monitor_search = RwSignal::new(String::new());
    let manual_check_busy = RwSignal::new(false);
    let manual_check_notice = RwSignal::new(None::<String>);

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
            match monitoring::fetch_overview().await {
                Ok(data) => {
                    overview.set(Some(data));
                    last_updated.set(Some(
                        js_sys::Date::new_0()
                            .to_iso_string()
                            .as_string()
                            .unwrap_or_else(|| "just now".to_string()),
                    ));
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

    spawn_local(async move {
        let _ = poll_data().await;
    });
    polling::start(poll_data);

    let relative_timer_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let relative_timer_cleanup = relative_timer_cancel.clone();
    on_cleanup(move || relative_timer_cleanup.store(true, std::sync::atomic::Ordering::Relaxed));
    spawn_local(async move {
        use gloo_timers::future::TimeoutFuture;
        loop {
            TimeoutFuture::new(1_000).await;
            if relative_timer_cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            relative_now.set(js_sys::Date::new_0().get_time());
        }
    });

    let on_retry = Callback::new(move |_: ()| {
        spawn_local(async move {
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

    let refresh_click = move |_| on_retry.run(());
    let run_check = move |_| {
        if manual_check_busy.get_untracked() {
            return;
        }
        manual_check_busy.set(true);
        manual_check_notice.set(None);
        spawn_local(async move {
            match monitoring::run_checks_now().await {
                Ok(result) => manual_check_notice.set(Some(format!(
                    "{} monitor{} queued for an immediate check.",
                    result.queued,
                    if result.queued == 1 { "" } else { "s" }
                ))),
                Err(e) => manual_check_notice.set(Some(user_facing_error(&e.to_string()))),
            }
            manual_check_busy.set(false);
            on_retry.run(());
        });
    };

    view! {
        <div class="dashboard-overview">
            <header class="dashboard-page-header">
                <div>
                    <p class="eyebrow">"SYSTEM OVERVIEW"</p>
                    <h1>
                        {move || {
                            let name = match auth.state.get() {
                                AuthState::Authenticated(user) => user.email.split('@').next().unwrap_or("there").to_string(),
                                _ => "there".to_string(),
                            };
                            let hour = js_sys::Date::new_0().get_hours();
                            let greeting = if hour < 12 { "Good morning" } else if hour < 18 { "Good afternoon" } else { "Good evening" };
                            format!("{greeting}, {name} 👋")
                        }}
                    </h1>
                    <p class="page-subtitle">"Here's what's happening with your services today."</p>
                    <p class="dashboard-last-updated">
                        {move || match last_updated.get() {
                            Some(value) => format!("Updated {}", format_timestamp(&value)),
                            None => "Waiting for live telemetry".to_string(),
                        }}
                    </p>
                </div>
                <div class="page-header-actions dashboard-actions">
                    <A href="/monitors/new" attr:class="btn btn-primary">"+ Add monitor"</A>
                    <A href="/status-pages" attr:class="btn">"Create status page"</A>
                    <button class="btn" type="button" on:click=run_check disabled=move || manual_check_busy.get()>
                        <span aria-hidden="true">"⚡"</span>
                        {move || if manual_check_busy.get() { "Queueing…" } else { "Run scheduled checks" }}
                    </button>
                    <button class="btn btn-ghost" type="button" on:click=refresh_click disabled=move || refreshing.get()>
                        {move || if refreshing.get() { "Refreshing…" } else { "Refresh" }}
                    </button>
                </div>
            </header>

            {move || manual_check_notice.get().map(|notice| view! { <div class="dashboard-action-notice" role="status">{notice}</div> })}

            {move || {
                if loading.get() && overview.get().is_none() {
                    view! { <LoadingSpinner text="Loading live dashboard data..."/> }.into_any()
                } else if error.get().is_some() && overview.get().is_none() {
                    let err = error.get().unwrap_or_else(|| "Unable to load dashboard.".to_string());
                    view! { <ErrorState title="Failed to load dashboard" description=err on_retry=on_retry/> }.into_any()
                } else if let Some(data) = overview.get() {
                    let summary = data.monitor_summary.clone();
                    let performance = data.performance.clone();
                    let incidents = data.incident_summary.recent.clone();
                    let open_incidents = data.incident_summary.open_count;
                    let trend = performance.trend.clone();
                    let uptime = performance.uptime_percentage.map(|v| format!("{}%", format_number(v))).unwrap_or_else(|| "—".to_string());
                    let latency = performance.avg_latency_ms.map(|v| format!("{} ms", format_number(v))).unwrap_or_else(|| "—".to_string());
                    let incident_total_24h: u32 = trend.iter().map(|point| point.incident_count).sum();
                    let spark_uptime = trend.iter().filter_map(|p| p.uptime_percentage).collect::<Vec<_>>();
                    let _spark_latency = trend.iter().filter_map(|p| p.avg_latency_ms).collect::<Vec<_>>();
                    let spark_incidents = trend.iter().map(|p| p.incident_count as f64).collect::<Vec<_>>();
                    let spark_monitors = vec![summary.total as f64; trend.len().max(2)];
                    let up_delta = trend_delta(&trend, "uptime");
                    let incident_delta = trend_delta(&trend, "incidents");

                    view! {
                        <div class="dashboard-content">
                            {move || error.get().map(|message| view! {
                                <div class="dashboard-refresh-error" role="alert">
                                    <span>{message}</span>
                                    <button class="btn" on:click=move |_| on_retry.clone().run(()) disabled=move || refreshing.get()>"Retry"</button>
                                </div>
                            })}

                            <section class="metric-strip" aria-label="Monitor health summary">
                                <div class="card metric-card metric-primary">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-monitors" aria-hidden="true">"◈"</span><span class="card-label">"MONITORS"</span></div>
                                    <span class="card-value">{summary.total}</span>
                                    <span class="card-hint"><span class="metric-trend-copy">"Configured"</span></span>
                                    <StatSparkline values=spark_monitors.clone()/>
                                </div>
                                <div class="card metric-card metric-up">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-up" aria-hidden="true">"✓"</span><span class="card-label">"UP"</span></div>
                                    <span class="card-value up">{summary.up}</span>
                                    <span class="card-hint"><TrendIndicator delta=up_delta.unwrap_or(0.0) /><span>"vs 24h ago"</span></span>
                                    <StatSparkline values=spark_uptime/>
                                </div>
                                <div class="card metric-card metric-down">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-down" aria-hidden="true">"!"</span><span class="card-label">"DOWN"</span></div>
                                    <span class="card-value down">{summary.down}</span>
                                    <span class="card-hint"><TrendIndicator delta=incident_delta.unwrap_or(0.0) inverse=true/><span>"vs 24h ago"</span></span>
                                    <StatSparkline values=spark_incidents.clone()/>
                                </div>
                                <div class="card metric-card metric-disabled">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-disabled" aria-hidden="true">"–"</span><span class="card-label">"DISABLED"</span></div>
                                    <span class="card-value">{summary.disabled}</span>
                                    <span class="card-hint">"Not monitored"</span>
                                    <StatSparkline values=spark_monitors.clone()/>
                                </div>
                                <div class="card metric-card metric-incident">
                                    <div class="metric-card-top"><span class="metric-icon metric-icon-incident" aria-hidden="true">"!"</span><span class="card-label">"INCIDENTS"</span></div>
                                    <span class="card-value">{open_incidents}</span>
                                    <span class="card-hint"><TrendIndicator delta=incident_delta.unwrap_or(0.0) inverse=true/><span>{format!("{} started in 24h", incident_total_24h)}</span></span>
                                    <StatSparkline values=spark_incidents.clone()/>
                                </div>
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading"><div><p class="eyebrow">"PERFORMANCE · 24H"</p><h2>"Performance"</h2></div></div>
                                <div class="dashboard-trend-grid">
                                    <TrendChart title="Uptime" value=uptime description="Observed successful checks" trend=trend.clone() metric="uptime" unit=""/>
                                    <TrendChart title="Response time" value=latency description="Observed check latency" trend=trend.clone() metric="latency" unit=""/>
                                    <TrendChart title="Incident activity" value=incident_total_24h.to_string() description="Incidents started in the last 24 hours" trend=trend.clone() metric="incidents" unit=""/>
                                </div>
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading dashboard-section-heading"><div><p class="eyebrow">"MONITORS"</p><h2>"Monitor health"</h2></div><A href="/monitors">"View all"</A></div>
                                <div class="monitor-toolbar">
                                    <div class="monitor-tabs" role="tablist" aria-label="Monitor status filter">
                                        {[
                                            ("all", format!("All ({})", summary.total)),
                                            ("up", format!("Up ({})", summary.up)),
                                            ("down", format!("Down ({})", summary.down)),
                                            ("disabled", format!("Disabled ({})", summary.disabled)),
                                        ].into_iter().map(|(key, label)| {
                                            let active = key.to_string();
                                            view! { <button type="button" class:active=move || monitor_filter.get() == active class="monitor-tab" on:click=move |_| monitor_filter.set(key.to_string()) role="tab">{label}</button> }
                                        }).collect_view()}
                                    </div>
                                    <input class="monitor-search" type="search" placeholder="Search monitors…" aria-label="Search monitors" prop:value=monitor_search on:input=move |event| monitor_search.set(event_target_value(&event))/>
                                </div>
                                {move || {
                                    let filter = monitor_filter.get();
                                    let search = monitor_search.get().trim().to_ascii_lowercase();
                                    let monitors = data.monitors.iter().filter(|monitor| {
                                        let status = monitor_status(monitor);
                                        let matches_filter = filter == "all" || status == filter;
                                        let matches_search = search.is_empty() || monitor.name.to_ascii_lowercase().contains(&search) || monitor.url.to_ascii_lowercase().contains(&search);
                                        matches_filter && matches_search
                                    }).take(RECENT_MONITORS_LIMIT).cloned().collect::<Vec<_>>();
                                    if monitors.is_empty() {
                                        view! { <div class="dashboard-empty compact"><p>"No monitors match this filter."</p></div> }.into_any()
                                    } else {
                                        view! { <div class="monitor-table-wrap"><table class="dashboard-monitor-table"><thead><tr><th>"Monitor"</th><th>"Status"</th><th>"Latency"</th><th>"Last check"</th><th></th></tr></thead><tbody>{monitors.into_iter().map(|monitor| {
                                            let id = monitor.id.clone();
                                            let status = monitor_status(&monitor);
                                            let (type_class, type_label) = monitor_type(&monitor.kind, &monitor.url);
                                            let latency = monitor.latency_ms.map(|v| format!("{v} ms")).unwrap_or_else(|| "—".to_string());
                                            let href = format!("/monitors/{id}");
                                            let checked = relative_time(monitor.checked_at.as_deref(), relative_now.get());
                                            view! { <tr class="monitor-row">
                                                <td><div class="monitor-cell"><span class=format!("monitor-type-icon monitor-type-{type_class}") aria-hidden="true"></span><A href=href.clone() attr:class="monitor-link"><strong>{monitor.name.clone()}</strong><small>{type_label}</small></A></div></td>
                                                <td><StatusBadge status=status/></td><td class="mono">{latency}</td><td class="mono relative-time">{checked}</td>
                                                <td class="monitor-row-end"><details class="row-menu"><summary aria-label="Monitor actions">"•••"</summary><div class="row-menu-popover"><A href=href.clone()>"Open monitor"</A><A href=format!("/monitors/{id}/edit")>"Edit"</A></div></details><A href=href attr:class="row-chevron" attr:aria-label="Open monitor">"›"</A></td>
                                            </tr> }
                                        }).collect_view()}</tbody></table></div> }.into_any()
                                    }
                                }}
                            </section>

                            <section class="dashboard-section">
                                <div class="section-heading dashboard-section-heading"><div><p class="eyebrow">"INCIDENTS"</p><h2>"Recent incidents"</h2></div><A href="/incidents">"View all"</A></div>
                                {if incidents.is_empty() { view! { <div class="dashboard-empty compact"><p>"No recent incidents."</p></div> }.into_any() } else { view! { <div class="incident-list dashboard-incident-list">{incidents.into_iter().map(|incident| {
    let incident_sparkline = spark_incidents.clone();
                                    let href = format!("/incidents/{}", incident.id);
                                    let _monitor_href = format!("/monitors/{}", incident.monitor_id);
                                    let resolved = incident.resolved_at.is_some();
                                    let status_label = if resolved { "Resolved" } else { "Investigating" };
                                    let status_class = if resolved { "incident-status-resolved" } else { "incident-status-investigating" };
                                    let when = relative_time(Some(&incident.started_at), relative_now.get());
                                    view! { <A href=href attr:class="incident-row incident-row-link">
                                        <span class=format!("incident-status-icon {}", status_class) aria-hidden="true">{if resolved { "✓" } else { "!" }}</span>
                                        <span class="incident-main"><strong>{incident.monitor_name.clone()}</strong><small>{format!("{} · {}", if resolved { "Resolved" } else { "Down" }, when)}</small></span>
                                        <StatusBadge status=if resolved { "up".to_string() } else { "down".to_string() } label=status_label.to_string()/>
                                        <StatSparkline values=incident_sparkline/>
                                        <span class="row-chevron" aria-hidden="true">"›"</span>
                                    </A> }
                                }).collect_view()}</div> }.into_any()}}
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
