use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn InvoicesPage() -> impl IntoView {
    let rows = RwSignal::new(Vec::<billing::Invoice>::new());
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            match billing::invoices().await {
                Ok(v) => rows.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    view! {<div class="feature-page"><header class="page-header"><div><span class="eyebrow">"BILLING"</span><h1>"Invoices"</h1></div></header>{move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}<section class="card"><div class="table-shell"><table class="data-table"><thead><tr><th>"Invoice"</th><th>"Period"</th><th>"Total"</th><th>"Status"</th><th>"Created"</th></tr></thead><tbody>{move||rows.get().into_iter().map(|i|view!{<tr><td>{i.invoice_number}</td><td>{format!("{} → {}",i.period_start,i.period_end)}</td><td>{format!("{} {}",i.currency,i.total)}</td><td>{i.status}</td><td>{i.created_at}</td></tr>}).collect_view()}</tbody></table></div></section></div>}
}
