use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(units: i64) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let a = units.unsigned_abs();
    format!("{sign}${}.{:02}", a / 1_000_000, (a % 1_000_000) / 10_000)
}

#[component]
pub fn WalletPage() -> impl IntoView {
    let wallet = RwSignal::new(None::<wallet::Wallet>);
    let transactions = RwSignal::new(Vec::<wallet::WalletTransaction>::new());
    let settings = RwSignal::new(None::<wallet::WalletSettings>);
    let error = RwSignal::new(None::<String>);
    let amount = RwSignal::new(25_i64);
    let loading = RwSignal::new(false);
    let reload = move || {
        spawn_local(async move {
            match wallet::get().await {
                Ok(v) => wallet.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            };
            match wallet::transactions().await {
                Ok(v) => transactions.set(v),
                Err(e) => error.set(Some(e.to_string())),
            };
            match wallet::settings().await {
                Ok(v) => settings.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            };
        });
    };
    Effect::new(move |_| reload());
    let top_up = move |_| {
        loading.set(true);
        let a = amount.get() * 1_000_000;
        spawn_local(async move {
            match wallet::top_up(&wallet::TopUpRequest {
                amount_units: a,
                payment_method: None,
            })
            .await
            {
                Ok(v) => {
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href(&v.checkout_url);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            };
            loading.set(false);
        });
    };
    view! { <div class="feature-page"><header class="page-header"><div><span class="eyebrow">"BILLING"</span><h1>"Wallet"</h1><p class="muted">"Prepaid PAYG balance for metered operations."</p></div></header>
    {move || error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}
    <section class="overview-cards">{move || match wallet.get(){Some(w)=>view!{<div class="card"><span class="card-label">"Balance"</span><span class="card-value">{usd(w.balance_units)}</span><span class="muted">{w.status}</span></div><div class="card"><span class="card-label">"Low balance"</span><span class="card-value">{usd(w.low_balance_threshold_units)}</span></div><div class="card"><span class="card-label">"Critical"</span><span class="card-value">{usd(w.critical_balance_threshold_units)}</span></div>}.into_any(),None=>view!{<div class="card"><span class="loading">"Loading wallet…"</span></div>}.into_any()}}</section>
    <section class="card"><div class="section-heading"><div><h2>"Top up"</h2><p class="muted">"Checkout is created server-side; the browser never supplies a price."</p></div></div><div class="inline-form">{[5_i64,10,25,50,100].into_iter().map(|v|{let cls=move || if amount.get()==v{"btn btn-primary"}else{"btn"}; view!{<button class=cls on:click=move |_| amount.set(v)>{format!("${v}")}</button>}}).collect_view()}<input type="number" min="1" max="1000" prop:value=move||amount.get().to_string() on:input=move|e|{if let Ok(v)=event_target_value(&e).parse(){amount.set(v)}}/><button class="btn btn-primary" disabled=move||loading.get() on:click=top_up>{move||if loading.get(){"Starting checkout…"}else{"Continue to payment"}}</button></div></section>
    <section class="card"><div class="section-heading"><h2>"Recent transactions"</h2><span class="muted">"Immutable ledger"</span></div><div class="table-shell"><table class="data-table"><thead><tr><th>"Date"</th><th>"Type"</th><th>"Description"</th><th>"Amount"</th><th>"Balance"</th></tr></thead><tbody>{move||transactions.get().into_iter().map(|t|view!{<tr><td>{t.created_at}</td><td>{t.transaction_type}</td><td>{t.description.unwrap_or_default()}</td><td>{usd(t.amount_units)}</td><td>{usd(t.balance_after_units)}</td></tr>}).collect_view()}</tbody></table></div></section>
    <section class="card"><h2>"Auto top-up"</h2>{move||match settings.get(){Some(s)=>view!{<p class="muted">{if s.auto_top_up_enabled{"Enabled"}else{"Disabled"}} {" · threshold "}{usd(s.auto_top_up_threshold_units)}{" · amount "}{usd(s.auto_top_up_amount_units)}</p>}.into_any(),None=>view!{<p class="muted">"Settings unavailable."</p>}.into_any()}}</section>
    </div> }
}
