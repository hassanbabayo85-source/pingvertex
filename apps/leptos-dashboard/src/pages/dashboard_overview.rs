//! Dashboard overview page.
//!
//! Notes:
//! - No time-range control: `GET /dashboard/overview` does not accept
//!   query params yet (see crates/api/src/routes/dashboard.rs), so a
//!   "Last 24h" style dropdown is intentionally omitted here. See
//!   HANDOVER-path-3.md, "Known gap".
//! - "Active Incidents" shows the aggregate count from `Overview` only.
//!   A full per-incident list is deferred to a future `/incidents` page
//!   backed by an aggregate endpoint, to avoid an N+1 per-monitor
//!   incident-fetch loop on the client.
//! - No chart: no charting library is available in this workspace yet.

use crate::api::monitoring::{self, Monitor, Overview};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::{PageHeader, PageHeaderProps};
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

const RECENT_MONITORS_LIMIT: usize = 5;

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let overview = RwSignal::new(Overview::default());
    let monitors = RwSignal::new(Vec::<Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    let load_data = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match monitoring::fetch_overview().await {
                Ok(data) => {
                    overview.set(data);
                    match monitoring::fetch_monitors().await {
                        Ok(data) => monitors.set(data),
                        Err(e) => error.set(Some(e)),
                    }
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    // Initial load on mount.
    Effect::new(move |_| {
        load_data();
    });

    let on_retry = Callback::new(move |_: ()| {
        load_data();
    });

    let refresh_click = move |_| load_data();

    // Use PageHeader component via builder to satisfy required `children` prop.
    // `PageHeader`'s `children: Option<Children>` (owned by Path 2, read-only
    // here) is not marked `#[prop(optional)]`, so it resolves to
    // `Option<Box<dyn FnOnce() -> AnyView + Send>>` rather than accepting a
    // bare closure directly — wrap accordingly instead of touching Path 2's
    // component file.
    let page_header = PageHeader(
        PageHeaderProps::builder()
            .title("Overview".to_string())
            .children(Some(Box::new(move || {
                view! {
                    <button class="btn" on:click=refresh_click>
                        "Refresh"
                    </button>
                }
                .into_any()
            })))
            .build(),
    );

    view! {
        <div class="dashboard-overview">
            {page_header}

            {move || {
                if loading.get() {
                    view! { <LoadingSpinner text="Loading overview..."/> }.into_any()
                } else if let Some(err) = error.get() {
                    view! {
                        <ErrorState
                            title="Failed to load dashboard"
                            description=err
                            on_retry=on_retry
                        />
                    }
                        .into_any()
                } else {
                    let o = overview.get();
                    let recent_monitors: Vec<Monitor> = monitors
                        .get()
                        .into_iter()
                        .take(RECENT_MONITORS_LIMIT)
                        .collect();

                    view! {
                        <div class="dashboard-content">
                            <div class="metric-strip">
                                <MetricCard label="Monitors" value=o.total_monitors.to_string()/>
                                <MetricCard label="Up" value=o.up_monitors.to_string()/>
                                <MetricCard label="Down" value=o.down_monitors.to_string()/>
                                <MetricCard
                                    label="Uptime"
                                    value=format!("{:.2}%", o.uptime_percentage)
                                />
                                <MetricCard
                                    label="Avg Response"
                                    value=format!("{:.0} ms", o.average_response_time_ms)
                                />
                                <MetricCard
                                    label="Active Incidents"
                                    value=o.active_incidents.to_string()
                                />
                            </div>

                            <div class="dashboard-widgets">
                                <section class="dashboard-widget monitor-status-widget">
                                    <h2>"Monitor Status"</h2>
                                    {if recent_monitors.is_empty() {
                                        view! {
                                            <p class="widget-empty">"No monitors yet."</p>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <ul class="monitor-status-list">
                                                {recent_monitors
                                                    .into_iter()
                                                    .map(|m| {
                                                        let status = m
                                                            .status
                                                            .clone()
                                                            .unwrap_or_else(|| "unknown".to_string());
                                                        view! {
                                                            <li class="monitor-status-item">
                                                                <span class="monitor-status-name">
                                                                    {m.name.clone()}
                                                                </span>
                                                                <StatusBadge status=status/>
                                                            </li>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </ul>
                                        }
                                            .into_any()
                                    }}
                                </section>

                                <section class="dashboard-widget active-incidents-widget">
                                    <h2>"Active Incidents"</h2>
                                    <p class="active-incidents-count">{o.active_incidents}</p>
                                    <A href="/monitors">"View monitors"</A>
                                </section>
                            </div>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}
