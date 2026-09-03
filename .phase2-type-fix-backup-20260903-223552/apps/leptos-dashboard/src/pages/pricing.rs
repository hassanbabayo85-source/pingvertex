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
        let monitors_value = match monitors.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some(
                    "Monitors must be a valid number of at least 1.".into(),
                ));
                return;
            }
        };
        let interval_value = match interval.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some(
                    "Interval must be a valid number of at least 1 second.".into(),
                ));
                return;
            }
        };
        let retention_value = match retention.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some(
                    "Retention must be a valid number of at least 1 day.".into(),
                ));
                return;
            }
        };
        let req = billing::QuoteRequest {
            monitors: monitors_value,
            interval_seconds: interval_value,
            retention_days: retention_value,
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
