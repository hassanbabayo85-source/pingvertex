use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(units: i64) -> String {
    // Unit is nano-USD (1 = $10^-9).
    let sign = if units < 0 { "-" } else { "" };
    let a = units.unsigned_abs();
    format!("{sign}${}.{:02}", a / 1_000_000_000, (a % 1_000_000_000) / 10_000_000)
}

#[component]
pub fn TransactionsPage() -> impl IntoView {
    let rows = RwSignal::new(Vec::<wallet::WalletTransaction>::new());
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match wallet::transactions().await {
                Ok(value) => {
                    error.set(None);
                    rows.set(value);
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
        });
    });

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"BILLING"</span>
                    <h1>"Transactions"</h1>
                    <p class="muted">
                        "Complete wallet ledger activity for this tenant."
                    </p>
                </div>
            </header>

            {move || {
                error.get().map(|message| {
                    view! {
                        <div class="error" role="alert">
                            {message}
                        </div>
                    }
                })
            }}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Wallet transactions"</h2>
                        <p class="muted">
                            "Top ups, PAYG usage, refunds, adjustments, promotional credits and expirations."
                        </p>
                    </div>
                </div>

                <div class="table-shell">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Timestamp"</th>
                                <th>"Type"</th>
                                <th>"Description"</th>
                                <th>"Amount"</th>
                                <th>"Balance after"</th>
                                <th>"Reference"</th>
                            </tr>
                        </thead>

                        <tbody>
                            {move || {
                                let values = rows.get();

                                if values.is_empty() {
                                    view! {
                                        <tr>
                                            <td colspan="6">
                                                <div class="empty-state">
                                                    <h3>"No transactions"</h3>
                                                    <p>
                                                        "Wallet activity will appear here after the first transaction."
                                                    </p>
                                                </div>
                                            </td>
                                        </tr>
                                    }
                                    .into_any()
                                } else {
                                    values
                                        .into_iter()
                                        .map(|transaction| {
                                            let reference = match (
                                                transaction.reference_type,
                                                transaction.reference_id,
                                            ) {
                                                (Some(kind), Some(id)) => {
                                                    format!("{kind}: {id}")
                                                }
                                                (Some(kind), None) => kind,
                                                (None, Some(id)) => id,
                                                (None, None) => "—".to_owned(),
                                            };

                                            view! {
                                                <tr>
                                                    <td>{transaction.created_at}</td>
                                                    <td>{transaction.transaction_type}</td>
                                                    <td>
                                                        {transaction.description.unwrap_or_else(|| "—".to_owned())}
                                                    </td>
                                                    <td>{usd(transaction.amount_units)}</td>
                                                    <td>{usd(transaction.balance_after_units)}</td>
                                                    <td><code>{reference}</code></td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                        .into_any()
                                }
                            }}
                        </tbody>
                    </table>
                </div>
            </section>
        </div>
    }
}
