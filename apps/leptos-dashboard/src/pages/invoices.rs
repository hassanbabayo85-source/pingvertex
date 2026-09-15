use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn InvoicesPage() -> impl IntoView {
    let rows = RwSignal::new(Vec::<billing::Invoice>::new());
    let error = RwSignal::new(None::<String>);

    // 🟡 MATAKI NA 3: Signals domin riƙe cikakken invoice da aka zaɓa
    let selected_invoice = RwSignal::new(None::<billing::Invoice>);
    let detail_error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            match billing::invoices().await {
                Ok(v) => rows.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });

    // 🟡 MATAKI NA 3: Function domin ɗauko cikakken bayanin invoice daga backend
    let load_detail = move |id: String| {
        busy.set(true);
        detail_error.set(None);
        spawn_local(async move {
            match billing::get_invoice_details(&id).await {
                Ok(inv) => selected_invoice.set(Some(inv)),
                Err(e) => detail_error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"BILLING"</span>
                    <h1>"Invoices"</h1>
                </div>
            </header>

            {move || error.get().map(|e| view! { <div class="error" role="alert">{e}</div> })}
            {move || detail_error.get().map(|e| view! { <div class="error" role="alert">{e}</div> })}

            // 🟡 MATAKI NA 3: Nuna Cikakken Bayani (Invoice Details View)
            {move || selected_invoice.get().map(|inv| {
                view! {
                    <div class="card" style="margin-bottom: 2rem; border-left: 4px solid var(--primary-color);">
                        <div style="display: flex; justify-content: space-between; align-items: flex-start;">
                            <h2>"Invoice Details: " {inv.invoice_number.clone()}</h2>
                            <button class="btn" on:click=move |_| selected_invoice.set(None)>"Close"</button>
                        </div>
                        <div style="margin-top: 1rem; display: grid; grid-template-columns: 1fr 1fr; gap: 1rem;">
                            <div>
                                <p class="muted">"Status"</p>
                                <p><strong>{inv.status.clone()}</strong></p>

                                <p class="muted">"Created At"</p>
                                <p>{inv.created_at.clone()}</p>

                                <p class="muted">"Paid At"</p>
                                <p>{inv.paid_at.clone().unwrap_or_else(|| "Not Paid".to_string())}</p>
                            </div>
                            <div>
                                <p class="muted">"Billing Period"</p>
                                <p>{inv.period_start.clone()} " → " {inv.period_end.clone()}</p>

                                <p class="muted">"Subtotal"</p>
                                <p>{inv.currency.clone()} " " {inv.subtotal.clone()}</p>

                                <p class="muted">"Tax"</p>
                                <p>{inv.currency.clone()} " " {inv.tax.clone()}</p>

                                <p class="muted">"Total"</p>
                                <p><strong>{inv.currency.clone()} " " {inv.total.clone()}</strong></p>
                            </div>
                        </div>
                    </div>
                }
            })}

            <section class="card">
                <div class="table-shell">
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"Invoice"</th>
                                <th>"Period"</th>
                                <th>"Total"</th>
                                <th>"Status"</th>
                                <th>"Created"</th>
                                <th>"Action"</th> // Ƙarin fili don maɓallin Action
                            </tr>
                        </thead>
                        <tbody>
                            {move || rows.get().into_iter().map(|i| {
                                let id = i.id.clone();
                                view! {
                                    <tr>
                                        <td>{i.invoice_number.clone()}</td>
                                        <td>{format!("{} → {}", i.period_start, i.period_end)}</td>
                                        <td>{format!("{} {}", i.currency, i.total)}</td>
                                        <td>{i.status.clone()}</td>
                                        <td>{i.created_at.clone()}</td>
                                        <td>
                                            <button class="btn btn-sm" on:click=move |_| load_detail(id.clone()) disabled=move || busy.get()>
                                                "View Details"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            </section>
        </div>
    }
}
