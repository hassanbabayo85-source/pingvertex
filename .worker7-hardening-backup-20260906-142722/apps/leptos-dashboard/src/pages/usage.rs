use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;
fn usd(u: i64) -> String {
    format!("${}.{:02}", u / 1_000_000, (u.abs() % 1_000_000) / 10_000)
}
#[component]
pub fn UsagePage() -> impl IntoView {
    let report = RwSignal::new(None::<wallet::UsageReport>);
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            match wallet::usage().await {
                Ok(v) => report.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    view! {<div class="feature-page"><header class="page-header"><div><span class="eyebrow">"PAYG"</span><h1>"Usage"</h1><p class="muted">"Metered usage, spend and projected monthly cost."</p></div></header>{move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}{move||match report.get(){Some(r)=>view!{<><section class="overview-cards"><div class="card"><span class="card-label">"Current balance"</span><span class="card-value">{usd(r.current_balance_units)}</span></div><div class="card"><span class="card-label">"Today's usage"</span><span class="card-value">{r.today_usage_units}</span></div><div class="card"><span class="card-label">"This month"</span><span class="card-value">{usd(r.month_spend_units)}</span></div><div class="card"><span class="card-label">"Projected spend"</span><span class="card-value">{usd(r.projected_spend_units)}</span></div></section><section class="card"><h2>"Daily usage"</h2><div class="table-shell"><table class="data-table"><thead><tr><th>"Day"</th><th>"Units"</th><th>"Spend"</th></tr></thead><tbody>{r.daily.into_iter().map(|d|view!{<tr><td>{d.day}</td><td>{d.usage_units}</td><td>{usd(d.spend_units)}</td></tr>}).collect_view()}</tbody></table></div></section><section class="card"><h2>"Usage by operation"</h2><div class="table-shell"><table class="data-table"><thead><tr><th>"Operation"</th><th>"Units"</th><th>"Spend"</th></tr></thead><tbody>{r.by_operation.into_iter().map(|o|view!{<tr><td>{o.operation}</td><td>{o.usage_units}</td><td>{usd(o.spend_units)}</td></tr>}).collect_view()}</tbody></table></div></section><section class="card"><h2>"Usage events"</h2><div class="table-shell"><table class="data-table"><thead><tr><th>"Timestamp"</th><th>"Operation"</th><th>"Units"</th><th>"Price"</th><th>"Request ID"</th><th>"Status"</th></tr></thead><tbody>{r.rows.into_iter().map(|e|view!{<tr><td>{e.timestamp}</td><td>{e.operation}</td><td>{e.units}</td><td>{usd(e.price_units)}</td><td>{e.request_id}</td><td>{e.status}</td></tr>}).collect_view()}</tbody></table></div></section></>}.into_any(),None=>view!{<div class="card"><span class="loading">"Loading usage…"</span></div>}.into_any()}}</div>}
}
