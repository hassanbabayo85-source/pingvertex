use crate::api::{billing, wallet};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(u: i64) -> String {
    // Unit is nano-USD (1 = $10^-9).
    format!("${}.{:06}", u / 1_000_000_000, (u.abs() % 1_000_000_000) / 1_000)
}

#[component]
pub fn BillingPage() -> impl IntoView {
    let w = RwSignal::new(None::<wallet::Wallet>);
    let p = RwSignal::new(Vec::<billing::PaygPricing>::new());
    let ft = RwSignal::new(None::<billing::FreeTier>);
    let min_top_up = RwSignal::new(None::<i64>);
    let allowance = RwSignal::new(None::<billing::Allowance>);
    let e = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match wallet::get().await {
                Ok(v) => w.set(Some(v)),
                Err(x) => e.set(Some(x.to_string())),
            };
            match billing::payg_pricing().await {
                Ok(page) => {
                    p.set(page.prices);
                    ft.set(Some(page.free_tier));
                    min_top_up.set(Some(page.payg_min_top_up_units));
                }
                Err(x) => e.set(Some(x.to_string())),
            }
            match billing::allowance().await {
                Ok(a) => allowance.set(Some(a)),
                Err(x) => e.set(Some(x.to_string())),
            }
        })
    });

    view! {
        <div class="feature-page billing-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Billing"</h1>
                    <p class="page-subtitle">"Prepaid usage. No subscription plans."</p>
                </div>
            </header>
            {move || e.get().map(|x| view! { <div class="error" role="alert">{x}</div> })}
            {move || w.get().map(|x| view! {
                <section class="card">
                    <div class="section-heading">
                        <div>
                            <h2>"Wallet"</h2>
                            <p class="muted">"Monitoring and billable operations consume wallet credit."</p>
                        </div>
                        <a class="btn btn-primary" href="/billing/wallet">"Add funds"</a>
                    </div>
                    <div class="overview-cards">
                        <div><span class="card-label">"Balance"</span><span class="card-value">{usd(x.balance_units)}</span></div>
                        <div><span class="card-label">"Available"</span><span class="card-value">{usd(x.available_units)}</span></div>
                        <div><span class="card-label">"Reserved"</span><span class="card-value">{usd(x.reserved_units)}</span></div>
                        <div><span class="card-label">"Status"</span><span class="card-value">{x.status}</span></div>
                        <div>
                            <span class="card-label">"Minimum top-up"</span>
                            <span class="card-value">
                                {move || min_top_up
                                    .get()
                                    .map(usd)
                                    .unwrap_or_else(|| "—".to_owned())}
                            </span>
                        </div>
                    </div>
                </section>
            })}
            {move || allowance.get().map(|a| view! {
                <section class="card">
                    <div class="section-heading">
                        <div>
                            <h2>"Fair-use alert allowance"</h2>
                            <p class="muted">{format!(
                                "Free SMS alerts included with your plan. Cycle ends {}.",
                                a.cycle_ends_at,
                            )}</p>
                        </div>
                    </div>
                    <div class="overview-cards">
                        <div>
                            <span class="card-label">"SMS used"</span>
                            <span class="card-value">{format!("{} / {}", a.sms_used, a.sms_limit)}</span>
                        </div>
                        <div>
                            <span class="card-label">"SMS remaining"</span>
                            <span class="card-value">{a.sms_remaining.to_string()}</span>
                        </div>
                        <div>
                            <span class="card-label">"Based on"</span>
                            <span class="card-value">{format!("{} enabled monitors", a.enabled_monitors)}</span>
                        </div>
                        <div>
                            <span class="card-label">"History retention"</span>
                            <span class="card-value">{format!("{} days", a.retention_days)}</span>
                        </div>
                        <div>
                            <span class="card-label">"Retention source"</span>
                            <span class="card-value">{a.retention_source.clone()}</span>
                        </div>
                        <div>
                            <span class="card-label">"Max retention"</span>
                            <span class="card-value">{format!("{} days", a.max_retention_days)}</span>
                        </div>
                    </div>
                    <p class="muted">"Once the free allowance is used, SMS falls back to paid SMS credits in your wallet, then to email if the wallet cannot pay."</p>
                    <p class="muted">"History retention controls how long check results are kept. It is applied per-tenant by the retention worker."</p>
                </section>
            })}
            <section class="card">
                <h2>"Price book"</h2>
                <p class="muted">"Prices are versioned and resolved by the server; clients cannot submit prices."</p>
                <div class="overview-cards">
                    {move || p.get().into_iter().map(|x| view! {
                        <div>
                            <span class="card-label">{x.operation}</span>
                            <span class="card-value">{usd(x.unit_price)}</span>
                            <span class="muted">{format!("per {} · v{}", x.unit_name, x.version)}</span>
                        </div>
                    }).collect_view()}
                </div>
            </section>
            <section class="card">
                <h2>"Free entitlement"</h2>
                {move || match ft.get() {
                    Some(x) => view! {
                        <p class="muted">{format!(
                            "Up to {} monitors at {} second minimum intervals, with {} days of history retention. \
                             More monitors or faster intervals require a funded wallet.",
                            x.max_monitors,
                            x.min_check_interval_seconds,
                            x.retention_days
                        )}</p>
                    }.into_any(),
                    None => view! { <p class="muted">"Loading entitlement…"</p> }.into_any()
                }}
            </section>
        </div>
    }
}
