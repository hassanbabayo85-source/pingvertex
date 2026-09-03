use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn PricingPage() -> impl IntoView {
    let monitors = RwSignal::new(String::from("1"));
    let interval = RwSignal::new("60".to_string());
    let retention = RwSignal::new("30".to_string());
    let quote = RwSignal::new(None::<billing::Quote>);
    let error = RwSignal::new(None::<String>);
    let refresh = move || {
        let req = billing::QuoteRequest {
            monitors: monitors.get().parse().unwrap_or(1),
            interval_seconds: interval.get().parse().unwrap_or(60),
            retention_days: retention.get().parse().unwrap_or(30),
        };
        spawn_local(async move {
            match billing::quote(&req).await {
                Ok(v) => quote.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Pricing"</h1></header><p class="muted">"Live prices below come directly from the backend pricing configuration."</p><form on:submit=move|e|{e.prevent_default();refresh()} class="inline-form"><input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/><input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/><input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/><button class="btn btn-primary">"Calculate"</button></form>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||quote.get().map(|q|view!{<section class="card"><h2>{format!("{} {} / month",q.monthly_price,q.currency)}</h2><p>{q.interval_label}  -  {q.retention_label}</p><p class="muted">{format!("{} checks per period",q.checks_per_period)}</p></section>})}</div>}
}
