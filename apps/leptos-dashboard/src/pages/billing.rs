use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_bill(role: &Role) -> bool {
    matches!(role, Role::Owner)
}
#[component]
pub fn BillingPage() -> impl IntoView {
    let auth = use_auth();
    let summary = RwSignal::new(None::<billing::BillingSummary>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let plan = RwSignal::new("tier_1".to_string());
    let session = RwSignal::new(String::new());
    let checkout_status = RwSignal::new(None::<String>);
    let quote = RwSignal::new(None::<billing::Quote>);
    let monitors = RwSignal::new(String::from("1"));
    let interval = RwSignal::new("60".to_string());
    let retention = RwSignal::new("30".to_string());
    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_bill(&u.role),
        _ => false,
    };
    Effect::new(move |_| {
        spawn_local(async move {
            match billing::summary().await {
                Ok(v) => summary.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    let get_quote = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let monitors_value = match monitors.get().trim().parse::<u64>() {
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
    let checkout = move || {
        let p = plan.get();
        spawn_local(async move {
            match billing::checkout(&p).await {
                Ok(v) => {
                    notice.set(Some(
                        "Checkout created. Continue in the payment provider.".into(),
                    ));
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href(&v.checkout_url);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Billing"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><h2>"Current billing"</h2>{move||summary.get().map(|s|view!{<div class="overview-cards"><div class="card"><span class="card-label">"Base"</span><span class="card-value">{format!("{} {}",s.base_price,s.currency)}</span></div><div class="card"><span class="card-label">"Add-ons"</span><span class="card-value">{format!("{} {}",s.add_ons_total,s.currency)}</span></div><div class="card"><span class="card-label">"Total"</span><span class="card-value">{format!("{} {}",s.total,s.currency)}</span></div></div>})}</section><section class="card"><h2>"Pricing quote"</h2><p class="muted">"The server is the only pricing authority; these values are sent to the quote endpoint and never calculated in the browser."</p><form on:submit=get_quote class="inline-form"><label>"Monitors"<input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/></label><label>"Interval (s)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/></label><label>"Retention (days)"<input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/></label><button class="btn">"Get quote"</button></form>{move||quote.get().map(|q|view!{<div class="quote-box"><strong>{format!("{} {} / month",q.monthly_price,q.currency)}</strong><span>{q.interval_label}</span><span>{q.retention_label}</span></div>})}</section><section class="card"><h2>"Verify checkout"</h2><p class="muted">"After the provider completes payment, enter the checkout session ID returned by the server. Payment state is verified server-side."</p><form on:submit=move|e|{e.prevent_default();let id=session.get();spawn_local(async move{match billing::verify_checkout(&id).await{Ok(v)=>checkout_status.set(Some(v.status)),Err(e)=>error.set(Some(e.to_string()))}})} class="inline-form"><input placeholder="Checkout session ID" prop:value=session on:input=move|e|session.set(event_target_value(&e)) required/><button class="btn">"Verify"</button></form>{move||checkout_status.get().map(|v|view!{<p><strong>{v}</strong></p>})}</section>{move||if can(){view!{<section class="card"><h2>"Change plan"</h2><div class="inline-form"><select prop:value=plan on:change=move|e|plan.set(event_target_value(&e))><option value="free">"Free"</option><option value="tier_1">"Tier 1"</option><option value="tier_5">"Tier 5"</option><option value="tier_10">"Tier 10"</option><option value="tier_20">"Tier 20"</option></select><button class="btn btn-primary" on:click=move |_|checkout()>"Continue to checkout"</button></div></section>}.into_any()}else{view!{<p class="muted">"Only the tenant owner can change the subscription."</p>}.into_any()}}</div>}
}
