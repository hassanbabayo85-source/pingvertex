use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(u: i64) -> String {
    let sign = if u < 0 { "-" } else { "" };
    let a = u.unsigned_abs();

    format!("{sign}${}.{:02}", a / 1_000_000, (a % 1_000_000) / 10_000)
}

fn date_start(v: &str) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!("{v}T00:00:00Z")
    }
}

fn date_end(v: &str) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!("{v}T23:59:59Z")
    }
}

fn chart_width(value: i64, maximum: i64) -> String {
    if maximum <= 0 || value <= 0 {
        "0%".to_owned()
    } else {
        let width = ((value as f64 / maximum as f64) * 100.0).round() as i64;
        format!("{}%", width.clamp(2, 100))
    }
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
            )
            .await
            {
                Ok(value) => {
                    error.set(None);
                    report.set(Some(value));
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
        });
    });

    let apply_filters = move |_| {
        reload.update(|value| *value += 1);
    };

    let clear_filters = move |_| {
        from.set(String::new());
        to.set(String::new());
        operation.set(String::new());
        status.set(String::new());
        reload.update(|value| *value += 1);
    };

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Usage"</h1>
                    <p class="muted">
                        "Metered usage, spend and projected monthly cost."
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
                        <h2>"Filters"</h2>
                        <p class="muted">
                            "Narrow the report without changing the underlying ledger."
                        </p>
                    </div>
                </div>

                <div class="inline-form">
                    <label>
                        "From "
                        <input
                            type="date"
                            prop:value=move || from.get()
                            on:input=move |event| {
                                from.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <label>
                        "To "
                        <input
                            type="date"
                            prop:value=move || to.get()
                            on:input=move |event| {
                                to.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <label>
                        "Operation "
                        <input
                            placeholder="proxy_check"
                            maxlength="100"
                            prop:value=move || operation.get()
                            on:input=move |event| {
                                operation.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <label>
                        "Status "
                        <select
                            prop:value=move || status.get()
                            on:change=move |event| {
                                status.set(event_target_value(&event));
                            }
                        >
                            <option value="">"All"</option>
                            <option value="SUCCEEDED">"Succeeded"</option>
                            <option value="FAILED">"Failed"</option>
                        </select>
                    </label>

                    <div class="inline-actions">
                        <button
                            type="button"
                            class="btn btn-primary"
                            on:click=apply_filters
                        >
                            "Apply"
                        </button>

                        <button
                            type="button"
                            class="btn"
                            on:click=clear_filters
                        >
                            "Clear"
                        </button>
                    </div>
                </div>
            </section>

            {move || {
                if let Some(r) = report.get() {
                    view! {
                        <section class="metric-grid">
                            <article class="card metric-card">
                                <span class="eyebrow">"CURRENT BALANCE"</span>
                                <strong>{usd(r.current_balance_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"TODAY'S USAGE"</span>
                                <strong>{usd(r.today_usage_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"THIS MONTH"</span>
                                <strong>{usd(r.month_usage_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"MONTH SPEND"</span>
                                <strong>{usd(r.month_spend_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"PROJECTED SPEND"</span>
                                <strong>{usd(r.projected_spend_units)}</strong>
                            </article>
                        </section>
                    }
                    .into_any()
                } else {
                    view! {
                        <section class="card">
                            <div class="loading">
                                "Loading usage report…"
                            </div>
                        </section>
                    }
                    .into_any()
                }
            }}

            {move || {
                if let Some(r) = report.get() {
                    if r.daily.is_empty() {
                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Usage"</h2>
                                        <p class="muted">
                                            "Usage units grouped by day."
                                        </p>
                                    </div>
                                </div>
                                <div class="empty-state">
                                    <h3>"No daily usage"</h3>
                                    <p>"No usage matched the selected filters."</p>
                                </div>
                            </section>
                        }
                        .into_any()
                    } else {
                        let max_usage = r
                            .daily
                            .iter()
                            .map(|item| item.usage_units)
                            .max()
                            .unwrap_or(0);

                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Usage"</h2>
                                        <p class="muted">
                                            "Usage units grouped by day."
                                        </p>
                                    </div>
                                </div>

                                <div class="usage-chart" role="img" aria-label="Daily usage chart">
                                    {r.daily
                                        .iter()
                                        .map(|item| {
                                            let width = chart_width(item.usage_units, max_usage);

                                            view! {
                                                <div class="usage-chart-row">
                                                    <div class="usage-chart-label">
                                                        {item.day.clone()}
                                                    </div>

                                                    <div class="usage-chart-track">
                                                        <div
                                                            class="usage-chart-bar"
                                                            style=move || format!("width: {width}")
                                                            title=format!(
                                                                "{}: {}",
                                                                item.day,
                                                                usd(item.usage_units)
                                                            )
                                                        ></div>
                                                    </div>

                                                    <div class="usage-chart-value">
                                                        {usd(item.usage_units)}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                        .into_any()
                    }
                } else {
                    ().into_any()
                }
            }}

            {move || {
                if let Some(r) = report.get() {
                    if r.daily.is_empty() {
                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Spend"</h2>
                                        <p class="muted">
                                            "PAYG spend grouped by day."
                                        </p>
                                    </div>
                                </div>
                                <div class="empty-state">
                                    <h3>"No daily spend"</h3>
                                    <p>"No spend matched the selected filters."</p>
                                </div>
                            </section>
                        }
                        .into_any()
                    } else {
                        let max_spend = r
                            .daily
                            .iter()
                            .map(|item| item.spend_units)
                            .max()
                            .unwrap_or(0);

                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Spend"</h2>
                                        <p class="muted">
                                            "PAYG spend grouped by day."
                                        </p>
                                    </div>
                                </div>

                                <div class="usage-chart" role="img" aria-label="Daily spend chart">
                                    {r.daily
                                        .iter()
                                        .map(|item| {
                                            let width = chart_width(item.spend_units, max_spend);

                                            view! {
                                                <div class="usage-chart-row">
                                                    <div class="usage-chart-label">
                                                        {item.day.clone()}
                                                    </div>

                                                    <div class="usage-chart-track">
                                                        <div
                                                            class="usage-chart-bar"
                                                            style=move || format!("width: {width}")
                                                            title=format!(
                                                                "{}: {}",
                                                                item.day,
                                                                usd(item.spend_units)
                                                            )
                                                        ></div>
                                                    </div>

                                                    <div class="usage-chart-value">
                                                        {usd(item.spend_units)}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                        .into_any()
                    }
                } else {
                    ().into_any()
                }
            }}

            {move || {
                if let Some(r) = report.get() {
                    if r.by_operation.is_empty() {
                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Usage by Operation"</h2>
                                        <p class="muted">
                                            "Usage grouped by metered operation."
                                        </p>
                                    </div>
                                </div>

                                <div class="empty-state">
                                    <h3>"No usage"</h3>
                                    <p>"No usage matched the selected filters."</p>
                                </div>
                            </section>
                        }
                        .into_any()
                    } else {
                        let max_usage = r
                            .by_operation
                            .iter()
                            .map(|item| item.usage_units)
                            .max()
                            .unwrap_or(0);

                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Usage by Operation"</h2>
                                        <p class="muted">
                                            "Usage and spend grouped by operation."
                                        </p>
                                    </div>
                                </div>

                                <div class="usage-chart" role="img" aria-label="Usage by operation chart">
                                    {r.by_operation
                                        .iter()
                                        .map(|item| {
                                            let width =
                                                chart_width(item.usage_units, max_usage);

                                            view! {
                                                <div class="usage-chart-row">
                                                    <div class="usage-chart-label">
                                                        {item.operation.clone()}
                                                    </div>

                                                    <div class="usage-chart-track">
                                                        <div
                                                            class="usage-chart-bar"
                                                            style=move || format!("width: {width}")
                                                            title=format!(
                                                                "{}: {} usage · {} spend",
                                                                item.operation,
                                                                usd(item.usage_units),
                                                                usd(item.spend_units)
                                                            )
                                                        ></div>
                                                    </div>

                                                    <div class="usage-chart-value">
                                                        {usd(item.spend_units)}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                        .into_any()
                    }
                } else {
                    ().into_any()
                }
            }}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Usage Ledger"</h2>
                        <p class="muted">
                            "Individual PAYG usage records returned by the server."
                        </p>
                    </div>
                </div>

                {move || {
                    if let Some(r) = report.get() {
                        if r.rows.is_empty() {
                            view! {
                                <div class="empty-state">
                                    <h3>"No usage records"</h3>
                                    <p>
                                        "No usage matched the selected filters."
                                    </p>
                                </div>
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="table-shell">
                                    <table class="data-table">
                                        <thead>
                                            <tr>
                                                <th>"Timestamp"</th>
                                                <th>"Operation"</th>
                                                <th>"Units"</th>
                                                <th>"Price"</th>
                                                <th>"Charge"</th>
                                                <th>"Request ID"</th>
                                                <th>"Status"</th>
                                            </tr>
                                        </thead>

                                        <tbody>
                                            {r.rows
                                                .iter()
                                                .map(|row| {
                                                    view! {
                                                        <tr>
                                                            <td>{row.timestamp.clone()}</td>
                                                            <td><code>{row.operation.clone()}</code></td>
                                                            <td>{usd(row.units)}</td>
                                                            <td>{usd(row.price_units)}</td>
                                                            <td>{usd(row.charge_units)}</td>
                                                            <td>
                                                                <code>{row.request_id.clone()}</code>
                                                            </td>
                                                            <td>
                                                                <span class="status-badge">
                                                                    {row.status.clone()}
                                                                </span>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                    } else {
                        view! {
                            <div class="loading">
                                "Loading usage ledger…"
                            </div>
                        }
                        .into_any()
                    }
                }}
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Usage Summary"</h2>
                        <p class="muted">
                            "Financial totals are calculated server-side from the PAYG ledger."
                        </p>
                    </div>
                </div>

                {move || {
                    if let Some(r) = report.get() {
                        view! {
                            <div class="list-stack">
                                <div class="list-row">
                                    <span>"Current balance"</span>
                                    <strong>{usd(r.current_balance_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Today's usage"</span>
                                    <strong>{usd(r.today_usage_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Month usage"</span>
                                    <strong>{usd(r.month_usage_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Month spend"</span>
                                    <strong>{usd(r.month_spend_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Projected spend"</span>
                                    <strong>{usd(r.projected_spend_units)}</strong>
                                </div>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div class="muted">"Loading…"</div> }.into_any()
                    }
                }}
            </section>
        </div>
    }
}
