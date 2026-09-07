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
    let subscription = RwSignal::new(None::<billing::Subscription>);
    let plans = RwSignal::new(Vec::<billing::Plan>::new());
    let cancel_busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let plan = RwSignal::new("starter".to_string());
    let session = RwSignal::new(String::new());
    let checkout_status = RwSignal::new(None::<String>);
    let quote = RwSignal::new(None::<billing::Quote>);
    let monitors = RwSignal::new("1".to_string());
    let interval = RwSignal::new("60".to_string());
    let retention = RwSignal::new("30".to_string());
    let quote_busy = RwSignal::new(false);
    let checkout_busy = RwSignal::new(false);
    let verify_busy = RwSignal::new(false);
    let initial_loading = RwSignal::new(true);

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_bill(&u.role),
        _ => false,
    };

    Effect::new(move |_| {
        if !can() {
            initial_loading.set(false);
            return;
        }
        spawn_local(async move {
            match billing::summary().await {
                Ok(v) => summary.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            match billing::subscription().await {
                Ok(v) => subscription.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match billing::plans().await {
                Ok(v) => plans.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            initial_loading.set(false);
        })
    });

    let get_quote = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let monitors_value = match monitors.get().trim().parse::<u64>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some("Monitors must be at least 1.".into()));
                return;
            }
        };
        let interval_value = match interval.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some("Interval must be at least 1 second.".into()));
                return;
            }
        };
        let retention_value = match retention.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some("Retention must be at least 1 day.".into()));
                return;
            }
        };
        quote_busy.set(true);
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
            quote_busy.set(false);
        });
    };

    let checkout = move |_| {
        if checkout_busy.get() {
            return;
        }
        error.set(None);
        let p = plan.get();
        if p == "free" {
            error.set(Some(
                "Free plan does not use checkout. Select a paid tier.".into(),
            ));
            return;
        }
        checkout_busy.set(true);
        spawn_local(async move {
            match billing::checkout(&p).await {
                Ok(v) => {
                    notice.set(Some(
                        "Checkout created. Continue securely in the payment provider.".into(),
                    ));
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href(&v.checkout_url);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            checkout_busy.set(false);
        });
    };

    let verify = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let id = session.get().trim().to_string();
        if id.is_empty() {
            error.set(Some("Checkout session ID is required.".into()));
            return;
        }
        verify_busy.set(true);
        spawn_local(async move {
            match billing::verify_checkout(&id).await {
                Ok(v) => checkout_status.set(Some(v.status)),
                Err(e) => error.set(Some(e.to_string())),
            }
            verify_busy.set(false);
        });
    };

    let cancel = move |_| {
        if cancel_busy.get() {
            return;
        }
        cancel_busy.set(true);
        error.set(None);
        spawn_local(async move {
            match billing::cancel_subscription(true).await {
                Ok(v) => {
                    subscription.set(Some(v));
                    notice.set(Some(
                        "Cancellation scheduled for the end of the current period.".into(),
                    ));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            cancel_busy.set(false);
        });
    };

    view! {
        <div class="feature-page billing-page">
            <header class="page-header"><div><span class="eyebrow">"Subscription"</span><h1>"Billing"</h1><p class="page-subtitle">"Manage your plan, checkout, and billing details in one place."</p></div></header>
            {move || if !can() {
                view! { <div class="empty-state" role="status">
                    <h2>"Billing access required"</h2>
                    <p>"Only the tenant owner can manage subscription and checkout settings."</p>
                </div> }.into_any()
            } else {
                ().into_any()
            }}
            {move || error.get().map(|e| view! { <div class="error" role="alert" aria-live="assertive">{e}</div> })}
            {move || notice.get().map(|n| view! { <div class="success" role="status" aria-live="polite">{n}</div> })}

            {move || if can() && initial_loading.get() {
                view! { <div class="loading" role="status" aria-live="polite">"Loading billing…"</div> }.into_any()
            } else {
                ().into_any()
            }}

            <section class="card">
                <div class="section-heading"><h2>"Current billing"</h2><span class="muted">"Pricing is authoritative on the server."</span></div>
                {move || match summary.get() {
                    Some(s) => view! { <div class="overview-cards">
                        <div class="card"><span class="card-label">"Base"</span><span class="card-value">{format!("{} {}",s.base_price,s.currency)}</span></div>
                        <div class="card"><span class="card-label">"Add-ons"</span><span class="card-value">{format!("{} {}",s.add_ons_total,s.currency)}</span></div>
                        <div class="card"><span class="card-label">"Total"</span><span class="card-value">{format!("{} {}",s.total,s.currency)}</span></div>
                    </div> }.into_any(),
                    None => view! { <div class="empty-state"><h3>"No billing summary"</h3><p>"Billing data is not available yet."</p></div> }.into_any(),
                }}
            </section>

            <section class="card">
                <div class="section-heading"><div><h2>"Subscription"</h2><p class="muted">"Tenant-scoped billing state from the backend."</p></div></div>
                {move || subscription.get().map(|s| view! { <div class="overview-cards"><div><span class="card-label">"Plan"</span><span class="card-value">{s.plan_tier.clone()}</span></div><div><span class="card-label">"Status"</span><span class="card-value">{s.status.clone()}</span></div><div><span class="card-label">"Period end"</span><span class="card-value">{s.current_period_ends_at.clone().unwrap_or_else(|| "—".into())}</span></div></div> })}
                {move || if can() && subscription.get().map(|s| s.status != "canceled" && !s.cancel_at_period_end).unwrap_or(false) { view!{<button class="btn" disabled=move||cancel_busy.get() on:click=cancel>{move||if cancel_busy.get(){"Scheduling…"}else{"Cancel at period end"}}</button>}.into_any() } else { ().into_any() }}
            </section>

            <section class="card">
                <div class="section-heading"><div><h2>"Plans"</h2><p class="muted">"Limits and prices are read from the active server configuration."</p></div></div>
                <div class="overview-cards">{move || plans.get().into_iter().map(|p| view!{<div><span class="card-label">{p.tier.clone()}</span><span class="card-value">{format!("{} {}",p.monthly_price,p.currency)}</span><span class="muted">{format!("{} monitors · {} min checks · {} days history", p.max_monitors, p.min_check_interval_seconds / 60, p.history_retention_days)}</span></div>}).collect_view()}</div>
            </section>

            <section class="card">
                <h2>"Pricing quote"</h2>
                <p class="muted">"Quotes are calculated by the backend; changing these values never charges your account."</p>
                <form on:submit=get_quote class="inline-form">
                    <label>"Monitors"<input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/></label>
                    <label>"Interval (s)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/></label>
                    <label>"Retention (days)"<input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/></label>
                    <button class="btn" disabled=move || quote_busy.get()>{move || if quote_busy.get() {"Calculating…"} else {"Get quote"}}</button>
                </form>
                {move || quote.get().map(|q| view! { <div class="quote-box"><strong>{format!("{} {} / month",q.monthly_price,q.currency)}</strong><span>{q.interval_label}</span><span>{q.retention_label}</span></div> })}
            </section>

            <section class="card">
                <h2>"Verify checkout"</h2>
                <p class="muted">"Verification asks the backend for the authoritative payment state."</p>
                <form on:submit=verify class="inline-form">
                    <input type="password" autocomplete="off" placeholder="Checkout session ID" prop:value=session on:input=move|e|session.set(event_target_value(&e)) required/>
                    <button class="btn" disabled=move || verify_busy.get()>{move || if verify_busy.get() {"Verifying…"} else {"Verify"}}</button>
                </form>
                {move || checkout_status.get().map(|v| view! { <p><strong>{v}</strong></p> })}
            </section>

            {move || if can() {
                view! { <section class="card">
                    <h2>"Change plan"</h2>
                    <p class="muted">"You will be redirected to the payment provider before any charge is finalized."</p>
                    <div class="inline-form">
                        <select prop:value=plan on:change=move|e|plan.set(event_target_value(&e))>
                            <option value="starter">"Starter"</option><option value="pro">"Pro"</option><option value="business">"Business"</option><option value="enterprise">"Enterprise"</option>
                        </select>
                        <button class="btn btn-primary" disabled=move || checkout_busy.get() on:click=checkout>{move || if checkout_busy.get() {"Starting checkout…"} else {"Continue to checkout"}}</button>
                    </div>
                </section> }.into_any()
            } else {
                view! { <section class="card"><p class="muted">"Only the tenant owner can change the subscription."</p></section> }.into_any()
            }}
        </div>
    }
}
