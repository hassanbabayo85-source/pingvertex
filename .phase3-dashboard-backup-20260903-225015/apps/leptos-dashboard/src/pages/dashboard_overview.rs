//! Dashboard overview page backed by the single aggregated dashboard contract.

use crate::api::monitoring::{self, DashboardOverview, MonitorOverviewItem};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::metric_card::MetricCard;
use crate::components::page_header::{PageHeader, PageHeaderProps};
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

const RECENT_MONITORS_LIMIT: usize = 5;

fn user_facing_error(message: &str) -> String {
    if message.starts_with("Network error:") {
        "Unable to reach PingVertex. Check your connection and try again.".to_string()
    } else if message.starts_with("Server error") {
        "PingVertex could not load the dashboard right now. Please try again.".to_string()
    } else {
        message.to_string()
    }
}

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let overview = RwSignal::new(None::<DashboardOverview>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    let load_data = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match monitoring::fetch_overview().await {
                Ok(data) => overview.set(Some(data)),
                Err(e) => error.set(Some(user_facing_error(&e))),
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| load_data());

    let on_retry = Callback::new(move |_: ()| load_data());
    let refresh_click = move |_| load_data();

    let page_header = PageHeader(
        PageHeaderProps::builder()
            .title("Overview".to_string())
            .children(Box::new(move || {
                view! { <button class="btn" on:click=refresh_click>"Refresh"</button> }.into_any()
            }))
            .build(),
    );

    view! {
        <div class="dashboard-overview">
            {page_header}
            {move || {
                if loading.get() {
                    view! { <LoadingSpinner text="Loading overview..."/> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <ErrorState title="Failed to load dashboard" description=err on_retry=on_retry/> }.into_any()
                } else if let Some(data) = overview.get() {
                    let summary = data.monitor_summary;
                    let recent_monitors: Vec<MonitorOverviewItem> = data.monitors.into_iter().take(RECENT_MONITORS_LIMIT).collect();
                    let open_incidents = data.incident_summary.open_count;
                    view! {
                        <div class="dashboard-content">
                            <div class="metric-strip">
                                <MetricCard label="Monitors" value=summary.total.to_string()/>
                                <MetricCard label="Up" value=summary.up.to_string()/>
                                <MetricCard label="Down" value=summary.down.to_string()/>
                                <MetricCard label="Disabled" value=summary.disabled.to_string()/>
                                <MetricCard label="Active Incidents" value=open_incidents.to_string()/>
                            </div>
                            <div class="dashboard-widgets">
                                <section class="dashboard-widget monitor-status-widget">
                                    <h2>"Monitor Status"</h2>
                                    {if recent_monitors.is_empty() {
                                        view! { <p class="widget-empty">"No monitors yet."</p> }.into_any()
                                    } else {
                                        view! {
                                            <ul class="monitor-status-list">
                                                {recent_monitors.into_iter().map(|m| {
                                                    let status = m.status.unwrap_or_else(|| if m.enabled { "unknown".to_string() } else { "disabled".to_string() });
                                                    view! {
                                                        <li class="monitor-status-item">
                                                            <span class="monitor-status-name">{m.name}</span>
                                                            <StatusBadge status=status/>
                                                        </li>
                                                    }
                                                }).collect_view()}
                                            </ul>
                                        }.into_any()
                                    }}
                                </section>
                                <section class="dashboard-widget active-incidents-widget">
                                    <h2>"Active Incidents"</h2>
                                    <p class="active-incidents-count">{open_incidents}</p>
                                    <A href="/monitors">"View monitors"</A>
                                </section>
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
