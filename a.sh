#!/usr/bin/env bash
set -euo pipefail

cd /home/abubakar-hassan-babayo/uptime-monitor

python3 - <<'PY'
from pathlib import Path

# ---- Dashboard wallet API ----
p = Path("apps/leptos-dashboard/src/api/wallet.rs")
s = p.read_text()

if "pub struct WalletNotification" not in s:
    s = s.replace(
        "pub struct WalletTransaction {",
        """pub struct WalletNotification {
    pub id: String,
    pub event_type: String,
    pub message: String,
    pub read_at: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RefundRequest {
    pub provider_payment_id: String,
    pub amount_units: i64,
    pub currency: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RefundResponse {
    pub provider_refund_id: String,
    pub status: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WalletTransaction {""",
    )

if "pub async fn notifications()" not in s:
    s = s.replace(
        """pub async fn transactions() -> Result<Vec<WalletTransaction>, ApiError> {
    client::get_json("/wallet/transactions").await
}""",
        """pub async fn transactions() -> Result<Vec<WalletTransaction>, ApiError> {
    client::get_json("/wallet/transactions").await
}
pub async fn notifications() -> Result<Vec<WalletNotification>, ApiError> {
    client::get_json("/wallet/notifications").await
}
pub async fn refund(req: &RefundRequest) -> Result<RefundResponse, ApiError> {
    client::post_json("/wallet/refund", req).await
}""",
    )

p.write_text(s)

# ---- Dashboard wallet page ----
p = Path("apps/leptos-dashboard/src/pages/wallet.rs")
s = p.read_text()

if "let notifications = RwSignal" not in s:
    s = s.replace(
        "    let transactions = RwSignal::new(Vec::<wallet::WalletTransaction>::new());\n",
        "    let transactions = RwSignal::new(Vec::<wallet::WalletTransaction>::new());\n"
        "    let notifications = RwSignal::new(Vec::<wallet::WalletNotification>::new());\n",
    )

if "let refund_payment_id = RwSignal" not in s:
    s = s.replace(
        "    let critical_threshold = RwSignal::new(1_i64);\n",
        "    let critical_threshold = RwSignal::new(1_i64);\n"
        "    let refund_payment_id = RwSignal::new(String::new());\n"
        "    let refund_amount = RwSignal::new(0_i64);\n",
    )

if "match wallet::notifications().await" not in s:
    s = s.replace(
        """            match wallet::transactions().await {
                Ok(v) => transactions.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::settings().await {""",
        """            match wallet::transactions().await {
                Ok(v) => transactions.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::notifications().await {
                Ok(v) => notifications.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            match wallet::settings().await {""",
    )

if '<h2>"Wallet notifications"</h2>' not in s:
    marker = """            <section class="card"><div class="section-heading"><h2>"Recent transactions"</h2><a class="btn" href="/billing/transactions">"View all transactions"</a></div><div class="table-shell"><table class="data-table"><thead><tr><th>"Date"</th><th>"Type"</th><th>"Description"</th><th>"Amount"</th><th>"Balance"</th></tr></thead><tbody>{move||transactions.get().into_iter().map(|t|view!{<tr><td>{t.created_at}</td><td>{t.transaction_type}</td><td>{t.description.unwrap_or_default()}</td><td>{usd(t.amount_units)}</td><td>{usd(t.balance_after_units)}</td></tr>}).collect_view()}</tbody></table></div></section>
"""
    insert = """            <section class="card">
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

"""
    if marker not in s:
        raise SystemExit("Could not locate the Recent transactions section in wallet.rs")
    s = s.replace(marker, insert + marker)

p.write_text(s)
PY

echo "===== FORMAT ====="
cargo fmt --all

echo "===== CLIPPY ====="
cargo clippy --workspace --all-targets -- -D warnings

echo "===== FULL TEST ====="
cargo test --workspace

echo
echo "===== WORKER 7 FINAL GATE ====="
echo "PASS: fmt + clippy + workspace tests"
echo "Worker 7 frontend acceptance changes are applied."
