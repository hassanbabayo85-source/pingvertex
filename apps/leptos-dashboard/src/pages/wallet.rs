use crate::api::wallet;
use leptos::prelude::*;

fn micro_usd(units: i64) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let abs = units.unsigned_abs();
    format!("{sign}${}.{:06}", abs / 1_000_000, abs % 1_000_000)
}

#[component]
pub fn WalletPage() -> impl IntoView {
    let wallet = RwSignal::new(None::<wallet::Wallet>);
    let transactions = RwSignal::new(Vec::<wallet::WalletTransaction>::new());
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match wallet::get().await {
                Ok(value) => wallet.set(Some(value)),
                Err(err) => error.set(Some(err.to_string())),
            }
            match wallet::transactions().await {
                Ok(value) => transactions.set(value),
                Err(err) => error.set(Some(err.to_string())),
            }
        });
    });

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Wallet"</h1>
                </div>
            </header>

            {move || error.get().map(|e| view! { <div class="error" role="alert">{e}</div> })}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Available balance"</h2>
                        <p class="muted">"PAYG charges are calculated server-side. Prices are never accepted from the browser."</p>
                    </div>
                </div>
                {move || match wallet.get() {
                    Some(w) => view! {
                        <div class="overview-cards">
                            <div class="card">
                                <span class="card-label">"Balance"</span>
                                <span class="card-value">{micro_usd(w.balance_units)}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Currency"</span>
                                <span class="card-value">{w.currency}</span>
                            </div>
                        </div>
                    }.into_any(),
                    None => view! { <p class="loading">"Loading wallet…"</p> }.into_any(),
                }}
            </section>

            <section class="card">
                <div class="section-heading">
                    <h2>"Recent transactions"</h2>
                    <span class="muted">"Immutable ledger"</span>
                </div>
                {move || if transactions.get().is_empty() {
                    view! { <p class="muted">"No transactions yet."</p> }.into_any()
                } else {
                    view! {
                        <div class="table-shell">
                            <table class="data-table">
                                <thead><tr><th>"Type"</th><th>"Amount"</th><th>"Balance"</th><th>"Date"</th></tr></thead>
                                <tbody>
                                    {transactions.get().into_iter().map(|t| view! {
                                        <tr>
                                            <td>{t.transaction_type}</td>
                                            <td>{micro_usd(t.amount_units)}</td>
                                            <td>{micro_usd(t.balance_after_units)}</td>
                                            <td>{t.created_at}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="card">
                <h2>"How PAYG works"</h2>
                <p class="muted">"Usage is reserved before a billable operation, captured only after success, and protected against duplicate requests and concurrent overspending."</p>
            </section>
        </div>
    }
}
