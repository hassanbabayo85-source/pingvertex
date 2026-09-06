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
    let notifications = RwSignal::new(Vec::<wallet::WalletNotification>::new());
    let settings = RwSignal::new(None::<wallet::WalletSettings>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let amount = RwSignal::new(25_i64);
    let loading = RwSignal::new(false);
    let auto_enabled = RwSignal::new(false);
    let threshold = RwSignal::new(5_i64);
    let auto_amount = RwSignal::new(20_i64);
    let payment_method = RwSignal::new(String::new());
    let daily_limit = RwSignal::new(100_i64);
    let monthly_limit = RwSignal::new(500_i64);
    let retry_limit = RwSignal::new(3_i32);
    let low_threshold = RwSignal::new(5_i64);
    let critical_threshold = RwSignal::new(1_i64);
    let refund_payment_id = RwSignal::new(String::new());
    let refund_amount = RwSignal::new(0_i64);

    let reload = move || {
        spawn_local(async move {
            match wallet::get().await {
                Ok(v) => wallet.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::transactions().await {
                Ok(v) => transactions.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::notifications().await {
                Ok(v) => notifications.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::settings().await {
                Ok(v) => {
                    auto_enabled.set(v.auto_top_up_enabled);
                    threshold.set(v.auto_top_up_threshold_units / 1_000_000);
                    auto_amount.set(v.auto_top_up_amount_units / 1_000_000);
                    payment_method.set(v.payment_method.clone().unwrap_or_default());
                    daily_limit.set(v.daily_top_up_limit_units / 1_000_000);
                    monthly_limit.set(v.monthly_top_up_limit_units / 1_000_000);
                    retry_limit.set(v.retry_limit);
                    low_threshold.set(v.low_balance_threshold_units / 1_000_000);
                    critical_threshold.set(v.critical_balance_threshold_units / 1_000_000);
                    settings.set(Some(v));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    Effect::new(move |_| reload());

    let save_settings = move |_| {
        error.set(None);
        notice.set(None);

        let low = low_threshold.get().max(0);
        let critical = critical_threshold.get().max(0);
        let auto_trigger = threshold.get().max(0);
        let auto_top_up_amount = auto_amount.get().clamp(1, 1000);
        let daily = daily_limit.get().max(1);
        let monthly = monthly_limit.get().max(1);
        let retries = retry_limit.get().clamp(0, 10);
        let payment = payment_method.get().trim().to_owned();

        if critical > low {
            error.set(Some(
                "Critical warning must be less than or equal to the low-balance warning."
                    .to_owned(),
            ));
            return;
        }

        if auto_enabled.get() && payment.is_empty() {
            error.set(Some(
                "A saved payment method is required when auto top-up is enabled.".to_owned(),
            ));
            return;
        }

        if auto_enabled.get() && auto_trigger <= 0 {
            error.set(Some(
                "Auto top-up trigger must be greater than $0 when auto top-up is enabled."
                    .to_owned(),
            ));
            return;
        }

        if daily < auto_top_up_amount {
            error.set(Some(
                "Daily top-up limit must be at least the auto top-up amount.".to_owned(),
            ));
            return;
        }

        if monthly < daily {
            error.set(Some(
                "Monthly top-up limit must be at least the daily top-up limit.".to_owned(),
            ));
            return;
        }

        loading.set(true);

        let req = wallet::WalletSettingsRequest {
            low_balance_threshold_units: low * 1_000_000,
            critical_balance_threshold_units: critical * 1_000_000,
            auto_top_up_enabled: auto_enabled.get(),
            auto_top_up_threshold_units: auto_trigger * 1_000_000,
            auto_top_up_amount_units: auto_top_up_amount * 1_000_000,
            payment_method: (!payment.is_empty()).then_some(payment),
            daily_top_up_limit_units: daily * 1_000_000,
            monthly_top_up_limit_units: monthly * 1_000_000,
            retry_limit: retries,
        };

        spawn_local(async move {
            match wallet::update_settings(&req).await {
                Ok(v) => {
                    auto_enabled.set(v.auto_top_up_enabled);
                    threshold.set(v.auto_top_up_threshold_units / 1_000_000);
                    auto_amount.set(v.auto_top_up_amount_units / 1_000_000);
                    payment_method.set(v.payment_method.clone().unwrap_or_default());
                    daily_limit.set(v.daily_top_up_limit_units / 1_000_000);
                    monthly_limit.set(v.monthly_top_up_limit_units / 1_000_000);
                    retry_limit.set(v.retry_limit);
                    low_threshold.set(v.low_balance_threshold_units / 1_000_000);
                    critical_threshold.set(v.critical_balance_threshold_units / 1_000_000);
                    settings.set(Some(v));
                    notice.set(Some("Wallet settings saved successfully.".to_owned()));
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }

            loading.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"BILLING"</span><h1>"Wallet"</h1><p class="muted">"Prepaid PAYG balance for metered operations."</p></div></header>
            {move||error.get().map(|e|view!{<div class="error" role="alert">{e}</div>})}
            {move||notice.get().map(|n|view!{<div class="success" role="status">{n}</div>})}

            {move||wallet.get().map(|w|view!{<section class="card">
                <div class="section-heading"><div><h2>"Wallet status"</h2><p class="muted">"PAYG stops safely at zero; thresholds only control warnings and automation."</p></div><strong>{w.status.clone()}</strong></div>
                <div class="overview-cards"><div><span class="card-label">"Balance"</span><span class="card-value">{usd(w.balance_units)}</span></div><div><span class="card-label">"Low"</span><span class="card-value">{usd(w.low_balance_threshold_units)}</span></div><div><span class="card-label">"Critical"</span><span class="card-value">{usd(w.critical_balance_threshold_units)}</span></div></div>
                {move || {
                    if w.status == "WARNING" {
                        view! {
                            <div class="notice">
                                "Your PAYG balance is low. Consider topping up."
                            </div>
                        }.into_any()
                    } else if w.status == "CRITICAL" {
                        view! {
                            <div class="warning">
                                "Your PAYG balance is critically low."
                            </div>
                        }.into_any()
                    } else if w.status == "BLOCKED" {
                        view! {
                            <div class="error">
                                "PAYG is blocked because your balance is zero."
                            </div>
                        }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
            </section>})}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Top up"</h2>
                        <p class="muted">"Choose an amount and continue to the secure payment provider."</p>
                    </div>
                </div>

                <div class="stack-form">
                    <div>
                        <span class="card-label">"Quick amount"</span>
                        <div class="inline-form">
                            {
                                [5_i64, 10, 25, 50, 100]
                                    .into_iter()
                                    .map(|v| {
                                        let selected = move || {
                                            if amount.get() == v {
                                                "btn btn-primary"
                                            } else {
                                                "btn"
                                            }
                                        };

                                        view! {
                                            <button
                                                type="button"
                                                class=selected
                                                disabled=move || loading.get()
                                                on:click=move |_| amount.set(v)
                                            >
                                                {format!("${v}")}
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            }
                        </div>
                    </div>

                    <label>
                        "Custom amount ($)"
                        <input
                            type="number"
                            min="1"
                            max="1000"
                            step="1"
                            prop:value=move || amount.get().to_string()
                            on:input=move |e| {
                                if let Ok(v) = event_target_value(&e).parse::<i64>() {
                                    amount.set(v.clamp(1, 1000));
                                }
                            }
                            disabled=move || loading.get()
                        />
                    </label>

                    <div class="inline-form">
                        <span class="muted">
                            {move || format!("Selected: ${}.00", amount.get().clamp(1, 1000))}
                        </span>

                        <button
                            type="button"
                            class="btn btn-primary"
                            disabled=move || loading.get()
                            on:click=move |_| {
                                loading.set(true);
                                error.set(None);

                                let a = amount.get().clamp(1, 1000) * 1_000_000;

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
                                            } else {
                                                error.set(Some(
                                                    "Unable to open the payment provider."
                                                        .to_owned(),
                                                ));
                                                loading.set(false);
                                            }
                                        }
                                        Err(e) => {
                                            error.set(Some(e.to_string()));
                                            loading.set(false);
                                        }
                                    }
                                });
                            }
                        >
                            {move || if loading.get() {
                                "Opening payment…"
                            } else {
                                "Continue to Payment"
                            }}
                        </button>
                    </div>

                    <p class="muted">
                        "Minimum $1.00 · Maximum $1,000.00. Your wallet is credited only after the provider webhook is verified."
                    </p>
                </div>
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Auto top-up"</h2>
                        <p class="muted">
                            "Automatically replenish the wallet when the balance falls below the configured trigger."
                        </p>
                    </div>

                    <strong>
                        {move || {
                            if auto_enabled.get() {
                                "ENABLED"
                            } else {
                                "DISABLED"
                            }
                        }}
                    </strong>
                </div>

                <div class="stack-form">
                    <label class="check-row">
                        <input
                            type="checkbox"
                            prop:checked=move || auto_enabled.get()
                            disabled=move || loading.get()
                            on:change=move |event| {
                                auto_enabled.set(
                                    event_target::<web_sys::HtmlInputElement>(&event).checked()
                                );
                            }
                        />
                        " Enable auto top-up"
                    </label>

                    <div class="inline-form">
                        <label>
                            "Trigger below ($)"
                            <input
                                type="number"
                                min="0"
                                prop:value=move || threshold.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        threshold.set(v.max(0));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Top-up amount ($)"
                            <input
                                type="number"
                                min="1"
                                max="1000"
                                prop:value=move || auto_amount.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        auto_amount.set(v.clamp(1, 1000));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Daily limit ($)"
                            <input
                                type="number"
                                min="1"
                                prop:value=move || daily_limit.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        daily_limit.set(v.max(1));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Monthly limit ($)"
                            <input
                                type="number"
                                min="1"
                                prop:value=move || monthly_limit.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        monthly_limit.set(v.max(1));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Retry limit"
                            <input
                                type="number"
                                min="0"
                                max="10"
                                prop:value=move || retry_limit.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i32>()
                                    {
                                        retry_limit.set(v.clamp(0, 10));
                                    }
                                }
                            />
                        </label>
                    </div>

                    <label>
                        "Saved payment method ID"
                        <input
                            type="password"
                            autocomplete="off"
                            prop:value=move || payment_method.get()
                            disabled=move || loading.get()
                            on:input=move |event| {
                                payment_method.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <p class="muted">
                        "A saved payment method is required to enable automatic charging. Daily, monthly and retry limits are enforced server-side."
                    </p>

                    <div class="inline-form">
                        <label>
                            "Low warning ($)"
                            <input
                                type="number"
                                min="0"
                                prop:value=move || low_threshold.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        low_threshold.set(v.max(0));
                                    }
                                }
                            />
                        </label>

                        <label>
                            "Critical warning ($)"
                            <input
                                type="number"
                                min="0"
                                prop:value=move || critical_threshold.get().to_string()
                                disabled=move || loading.get()
                                on:input=move |event| {
                                    if let Ok(v) =
                                        event_target_value(&event).parse::<i64>()
                                    {
                                        critical_threshold.set(v.max(0));
                                    }
                                }
                            />
                        </label>

                        <button
                            class="btn btn-primary"
                            disabled=move || loading.get()
                            on:click=save_settings
                        >
                            {move || {
                                if loading.get() {
                                    "Saving…"
                                } else {
                                    "Save settings"
                                }
                            }}
                        </button>
                    </div>
                </div>
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Wallet notifications"</h2>
                        <p class="muted">"Low-balance, critical-balance and payment events for this wallet."</p>
                    </div>
                </div>
                {move || if notifications.get().is_empty() {
                    view! { <p class="muted">"No wallet notifications yet."</p> }.into_any()
                } else {
                    view! {
                        <div class="stack-form">
                            {move || notifications.get().into_iter().map(|n| view! {
                                <div class="notice">
                                    <strong>{n.event_type}</strong>
                                    <div>{n.message}</div>
                                    <small class="muted">{n.created_at}</small>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Request a refund"</h2>
                        <p class="muted">"Refund a verified wallet top-up. Refunds are processed by the payment provider and finalized by webhook verification."</p>
                    </div>
                </div>
                <div class="stack-form">
                    <label>
                        "Provider payment ID"
                        <input
                            type="text"
                            autocomplete="off"
                            prop:value=move || refund_payment_id.get()
                            on:input=move |event| refund_payment_id.set(event_target_value(&event))
                            disabled=move || loading.get()
                        />
                    </label>
                    <label>
                        "Refund amount ($)"
                        <input
                            type="number"
                            min="0.01"
                            max="1000"
                            step="0.01"
                            prop:value=move || {
                                let units = refund_amount.get();
                                format!("{}.{:02}", units / 1_000_000, (units % 1_000_000) / 10_000)
                            }
                            on:input=move |event| {
                                let raw = event_target_value(&event);
                                if let Ok(value) = raw.parse::<f64>() {
                                    refund_amount.set((value * 1_000_000.0).round() as i64);
                                }
                            }
                            disabled=move || loading.get()
                        />
                    </label>
                    <button
                        type="button"
                        class="btn btn-primary"
                        disabled=move || loading.get() || refund_payment_id.get().trim().is_empty() || refund_amount.get() <= 0
                        on:click=move |_| {
                            error.set(None);
                            notice.set(None);
                            loading.set(true);
                            let req = wallet::RefundRequest {
                                provider_payment_id: refund_payment_id.get().trim().to_owned(),
                                amount_units: refund_amount.get(),
                                currency: Some("USD".to_owned()),
                            };
                            spawn_local(async move {
                                match wallet::refund(&req).await {
                                    Ok(v) => {
                                        notice.set(Some(format!("Refund {} submitted; provider status: {}.", v.provider_refund_id, v.status)));
                                        refund_payment_id.set(String::new());
                                        refund_amount.set(0);
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                loading.set(false);
                            });
                        }
                    >
                        {move || if loading.get() { "Submitting…" } else { "Submit refund" }}
                    </button>
                    <p class="muted">"Refunds require billing permission and must match a refundable payment owned by this tenant."</p>
                </div>
            </section>

            <section class="card"><div class="section-heading"><h2>"Recent transactions"</h2><a class="btn" href="/billing/transactions">"View all transactions"</a></div><div class="table-shell"><table class="data-table"><thead><tr><th>"Date"</th><th>"Type"</th><th>"Description"</th><th>"Amount"</th><th>"Balance"</th></tr></thead><tbody>{move||transactions.get().into_iter().map(|t|view!{<tr><td>{t.created_at}</td><td>{t.transaction_type}</td><td>{t.description.unwrap_or_default()}</td><td>{usd(t.amount_units)}</td><td>{usd(t.balance_after_units)}</td></tr>}).collect_view()}</tbody></table></div></section>
        </div>
    }
}
