use crate::api::monitoring::{self, DashboardOverview};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

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

#[component]
pub fn IncidentsPage() -> impl IntoView {
    let data = RwSignal::new(None::<DashboardOverview>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    spawn_local(async move {
        match monitoring::fetch_overview().await {
            Ok(value) => {
                data.set(Some(value));
                error.set(None);
            }
            Err(err) => error.set(Some(err)),
        }
        loading.set(false);
    });

    view! {
        <div class="feature-page incidents-page">
            <header class="page-header">
                <div>
                    <p class="eyebrow">"MONITORING"</p>
                    <h1>"Incidents"</h1>
                    <p class="page-subtitle">"Recent incidents across your workspace."</p>
                </div>
                <A href="/monitors" attr:class="btn">"Monitors"</A>
            </header>
            {move || if loading.get() {
                view! { <LoadingSpinner text="Loading incidents..."/> }.into_any()
            } else if let Some(message) = error.get() {
                view! { <ErrorState title="Unable to load incidents" description=message/> }.into_any()
            } else if let Some(value) = data.get() {
                if value.incident_summary.recent.is_empty() {
                    view! { <div class="dashboard-empty"><h3>"No recent incidents"</h3><p>"Everything is quiet right now."</p></div> }.into_any()
                } else {
                    view! {
                        <section class="card incidents-table-card">
                            <div class="table-shell">
                                <table>
                                    <thead><tr><th>"Incident"</th><th>"Monitor"</th><th>"Started"</th><th>"Status"</th></tr></thead>
                                    <tbody>
                                        {value.incident_summary.recent.into_iter().map(|incident| {
                                            let incident_id = incident.id.clone();
                                            let monitor_id = incident.monitor_id.clone();
                                            let status = if incident.acknowledged_at.is_some() { "acknowledged" } else { "open" };
                                            view! {
                                                <tr>
                                                    <td><A href=format!("/incidents/{incident_id}") attr:class="incident-link mono">{incident_id}</A></td>
                                                    <td><A href=format!("/monitors/{monitor_id}") attr:class="monitor-link mono">{monitor_id}</A></td>
                                                    <td class="mono">{format_timestamp(&incident.started_at)}</td>
                                                    <td><StatusBadge status=status/></td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </section>
                    }.into_any()
                }
            } else {
                view! { <ErrorState title="Incidents unavailable" description="No incident data is currently available."/> }.into_any()
            }}
        </div>
    }
}
