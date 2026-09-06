use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(u: i64) -> String {
    let sign = if u < 0 { "-" } else { "" };
    let a = u.unsigned_abs();
    format!("{sign}${}.{:02}", a / 1_000_000, (a % 1_000_000) / 10_000)
}

fn date_start(v: &str) -> String {
    if v.is_empty() { String::new() } else { format!("{v}T00:00:00Z") }
}
fn date_end(v: &str) -> String {
    if v.is_empty() { String::new() } else { format!("{v}T23:59:59Z") }
}

#[component]
pub fn UsagePage() -> impl IntoView {
    let report = RwSignal::new(None::<wallet::UsageReport>);
    let error = RwSignal::new(None::<String>);
    let from = RwSignal::new(String::new());
    let to = RwSignal::new(String::new());
    let operation = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());
    let reload = RwSignal::new(0_u32);

    Effect::new(move |_| {
        let _ = reload.get();
        let f = date_start(&from.get());
        let t = date_end(&to.get());
        let op = operation.get();
        let st = status.get();
        spawn_local(async move {
            match wallet::usage(
                (!f.is_empty()).then_some(f.as_str()),
                (!t.is_empty()).then_some(t.as_str()),
                (!op.is_empty()).then_some(op.as_str()),
                (!st.is_empty()).then_some(st.as_str()),
            ).await {
                Ok(v) => { error.set(None); report.set(Some(v)); }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div><span class="eyebrow">"PAYG"</span><h1>"Usage"</h1><p class="muted">"Metered usage, spend and projected monthly cost."</p></div>
            </header>
            {move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}

            <section class="card">
                <div class="section-heading"><div><h2>"Filters"</h2><p class="muted">"Narrow the report without changing the underlying ledger."</p></div></div>
                <div class="inline-form">
                    <label>"From "<input type="date" prop:value=move||from.get() on:input=move|e|from.set(event_target_value(&e))/></label>
                    <label>"To "<input type="date" prop:value=move||to.get() on:input=move|e|to.set(event_target_value(&e))/></label>
                    <label>"Operation "<input placehol
