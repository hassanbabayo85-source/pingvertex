use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;
fn usd(u: i64) -> String {
    format!("${}.{:06}", u / 1_000_000, u.abs() % 1_000_000)
}
#[component]
pub fn PricingPage() -> impl IntoView {
    let prices = RwSignal::new(Vec::<billing::PaygPricing>::new());
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            match billing::payg_pricing().await {
                Ok(v) => prices.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    view! {<div class="feature-page"><header class="page-header"><div><span class="eyebrow">"PAYG"</span><h1>"Pricing"</h1><p class="muted">"Server-controlled prices per actual billable unit."</p></div></header>{move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}<section class="card"><div class="overview-cards">{move||prices.get().into_iter().map(|p|view!{<div><span class="card-label">{p.operation}</span><span class="card-value">{usd(p.unit_price)}</span><span class="muted">{format!("per {} · v{}",p.unit_name,p.version)}</span></div>}).collect_view()}</div></section><section class="card"><h2>"Free entitlement"</h2><p class="muted">"10 monitors at a 5-minute minimum interval. A funded wallet unlocks PAYG capacity without a product monitor-count ceiling."</p></section></div>}
}
