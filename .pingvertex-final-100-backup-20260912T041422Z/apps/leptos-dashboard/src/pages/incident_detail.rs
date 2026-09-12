use crate::api::monitoring::{self, Incident};
use crate::components::error_state::ErrorState;
use crate::components::loading::LoadingSpinner;
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

fn status_for(incident: &Incident) -> &'static str {
    if incident.resolved_at.is_some() {
        "resolved"
    } else if incident.acknowledged_at.is_some() {
        "acknowledged"
    } else {
        "open"
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

#[component]
pub fn IncidentDetailPage() -> impl IntoView {
    let params = use_params_map();
    let incident_id = move || params.get().get("id").unwrap_or_default();
    let incident = RwSignal::new(None::<Incident>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let in_flight = RwSignal::new(false);

    let load = move || {
        let id = incident_id();
        if id.is_empty() {
            error.set(Some("Invalid incident URL.".into()));
            loading.set(false);
            return;
        }
        if in_flight.get_untracked() {
            return;
        }
        in_flight.set(true);
        let incident = incident;
        let loading = loading;
        let error = error;
        let in_flight = in_flight;
        spawn_local(async move {
            match monitoring::fetch_incident(&id).await {
                Ok(value) => {
                    incident.set(Some(value));
                    error.set(None);
                }
                Err(err) => error.set(Some(err)),
            }
            loading.set(false);
            in_flight.set(false);
        });
    };

    load();

    view! {
        <div class="feature-page incident-detail-page">
            <header class="page-header">
                <div>
                    <p class="eyebrow">"INCIDENT"</p>
                    <h1>"Incident details"</h1>
                    <p class="page-subtitle">"Timeline and current state for this incident."</p>
                </div>
                <A href="/incidents" attr:class="btn">"All incidents"</A>
            </header>

            {move || if loading.get() && incident.get().is_none() {
                view! { <LoadingSpinner text="Loading incident..."/> }.into_any()
            } else if let Some(message) = error.get().filter(|_| incident.get().is_none()) {
                view! { <ErrorState title="Incident unavailable" description=message/> }.into_any()
            } else if let Some(value) = incident.get() {
                let status = status_for(&value);
                view! {
                    <section class="card incident-detail-card">
                        <div class="incident-detail-title">
                            <div>
                                <span class="muted mono">{value.id.clone()}</span>
                                <h2>{format!("Monitor {}", value.monitor_id)}</h2>
                            </div>
                            <StatusBadge status=status.to_string()/>
                        </div>
                        <dl class="detail-grid">
                            <div><dt>"First seen"</dt><dd>{format_timestamp(&value.first_seen)}</dd></div>
                            <div><dt>"Last seen"</dt><dd>{value.last_seen_at.as_deref().map(format_timestamp).unwrap_or_else(|| "—".into())}</dd></div>
                            <div><dt>"Resolved"</dt><dd>{value.resolved_at.as_deref().map(format_timestamp).unwrap_or_else(|| "Still open".into())}</dd></div>
                            <div><dt>"Duration"</dt><dd>{format!("{} ms", value.duration_ms)}</dd></div>
                            <div><dt>"Acknowledged"</dt><dd>{value.acknowledged_at.as_deref().map(format_timestamp).unwrap_or_else(|| "No".into())}</dd></div>
                            <div><dt>"Maintenance"</dt><dd>{if value.occurred_during_maintenance { "Yes" } else { "No" }}</dd></div>
                        </dl>
                        <div class="incident-detail-actions">
                            <A href=format!("/monitors/{}", value.monitor_id) attr:class="btn btn-primary">"Open monitor"</A>
                        </div>
                    </section>
                }.into_any()
            } else {
                view! { <ErrorState title="Incident unavailable" description="No incident data is currently available."/> }.into_any()
            }}
        </div>
    }
}
