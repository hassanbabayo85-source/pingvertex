use crate::public_api::{
    get_public_status, get_public_status_by_subdomain, Incident, PublicMonitor,
    PublicStatusResponse,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[component]
pub fn StatusPage() -> impl IntoView {
    let status_data = LocalResource::new(|| async move { get_public_status().await });

    view! {
        <div class="status-page">
            <div class="status-page-inner">
                <h1>"Status"</h1>
                {move || match status_data.get() {
                    None => view! { <p class="loading">"Loading..."</p> }.into_any(),
                    Some(wrapped) => match wrapped.take() {
                        Err(e) => view! {
                            <div class="error">{format!("Error loading status: {e}")}</div>
                        }.into_any(),
                        Ok(data) => view! { <StatusOverview data=data.clone()/> }.into_any(),
                    },
                }}
            </div>
        </div>
    }
}

#[component]
pub fn SubdomainStatusPage() -> impl IntoView {
    let params = use_params_map();
    let subdomain = move || params.with(|p| p.get("subdomain").unwrap_or_default());

    let status_data = LocalResource::new(move || {
        let sub = subdomain();
        async move {
            if sub.is_empty() {
                Err("Missing subdomain".to_string())
            } else {
                get_public_status_by_subdomain(&sub).await
            }
        }
    });

    view! {
        <div class="status-page">
            <div class="status-page-inner">
                <h1>"Status: " {move || params.with(|p| p.get("subdomain").unwrap_or_default())}</h1>
                {move || match status_data.get() {
                    None => view! { <p class="loading">"Loading..."</p> }.into_any(),
                    Some(wrapped) => match wrapped.take() {
                        Err(e) => view! {
                            <div class="error">{format!("Error loading status: {e}")}</div>
                        }.into_any(),
                        Ok(data) => view! { <StatusOverview data=data.clone()/> }.into_any(),
                    },
                }}
            </div>
        </div>
    }
}

#[component]
fn StatusOverview(data: PublicStatusResponse) -> impl IntoView {
    let overall = data.overall_status.clone();
    let (variant, label) = match overall.as_str() {
        "operational" => ("operational", "All Systems Operational"),
        "degraded" => ("degraded", "Degraded Performance"),
        "outage" => ("outage", "Major Outage"),
        _ => ("unknown", "Unknown Status"),
    };

    view! {
        <div class="status-hero" data-variant=variant>
            <span class="status-dot" data-variant=variant></span>
            <span class="status-hero-label">{label}</span>
        </div>

        <section class="status-section">
            <h2>"Services"</h2>
            <ul class="service-list">
                {data.monitors.iter().map(|monitor| view! { <ServiceRow monitor=monitor.clone()/> }).collect::<Vec<_>>()}
            </ul>
        </section>

        <section class="status-section">
            <h2>"Incident History"</h2>
            <ul class="incident-list">
                {data.incidents.iter().map(|incident| view! { <IncidentRow incident=incident.clone()/> }).collect::<Vec<_>>()}
            </ul>
        </section>
    }
}

#[component]
fn ServiceRow(monitor: PublicMonitor) -> impl IntoView {
    let variant = match monitor.status.as_str() {
        "up" => "operational",
        "down" => "outage",
        "paused" => "paused",
        _ => "degraded",
    };
    view! {
        <li class="service-row">
            <div class="service-row-main">
                <span class="status-dot" data-variant=variant></span>
                <div>
                    <p class="service-name">{monitor.name.clone()}</p>
                    <p class="service-url">{monitor.url.clone()}</p>
                </div>
            </div>
            <div class="service-uptime">{format!("{:.2}% uptime", monitor.uptime)}</div>
        </li>
    }
}

#[component]
fn IncidentRow(incident: Incident) -> impl IntoView {
    view! {
        <li class="incident-row">
            <div>
                <p class="incident-title">{incident.title.clone()}</p>
                <p class="incident-description">{incident.description.clone()}</p>
            </div>
            <div class="incident-date">{incident.created_at.clone()}</div>
        </li>
    }
}
