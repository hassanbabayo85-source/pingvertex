use crate::api::monitoring::{self, Overview};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

#[component]
pub fn DashboardOverviewPage() -> impl IntoView {
    let overview = RwSignal::new(Overview::default());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    Effect::new(move |_| {
        spawn_local(async move {
            match monitoring::fetch_overview().await {
                Ok(data) => overview.set(data),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    view! {
        <div class="dashboard-overview">
            <header class="page-header">
                <h1>"Dashboard Overview"</h1>
                <button on:click=move |_| navigate("/monitors", Default::default())>
                    "Manage Monitors"
                </button>
            </header>
            {move || {
                if loading.get() {
                    view! { <div class="loading">"Loading overview..."</div> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <div class="error">{err}</div> }.into_any()
                } else {
                    let o = overview.get();
                    view! {
                        <div class="overview-cards">
                            <div class="card">
                                <span class="card-label">"Total Monitors"</span>
                                <span class="card-value">{o.total_monitors}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Up"</span>
                                <span class="card-value up">{o.up_monitors}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Down"</span>
                                <span class="card-value down">{o.down_monitors}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Uptime %"</span>
                                <span class="card-value">{format!("{:.2}", o.uptime_percentage)}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Avg Response Time (ms)"</span>
                                <span class="card-value">{format!("{:.0}", o.average_response_time_ms)}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Active Incidents"</span>
                                <span class="card-value">{o.active_incidents}</span>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
