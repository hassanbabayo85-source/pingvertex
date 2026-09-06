use application::ports::{
    PaymentProvider, PaymentVerification, WalletRepository, WalletTopUpRequest,
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub struct AutoTopUpWorker {
    wallet: Arc<dyn WalletRepository>,
    provider: Arc<dyn PaymentProvider>,
    interval: Duration,
}

impl AutoTopUpWorker {
    pub fn new(
        wallet: Arc<dyn WalletRepository>,
        provider: Arc<dyn PaymentProvider>,
        interval: Duration,
    ) -> Self {
        Self {
            wallet,
            provider,
            interval,
        }
    }

    pub async fn run_forever(
        &self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut ticker = tokio::time::interval(self.interval);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Ok(());
                }

                _ = ticker.tick() => {
                    let candidates = self.wallet.auto_top_up_candidates(50).await?;

                    for candidate in candidates {
                        if candidate.daily_attempts >= candidate.settings.retry_limit {
                            continue;
                        }

                        if candidate.daily_spend_units
                            >= candidate.settings.daily_top_up_limit_units
                        {
                            continue;
                        }

                        if candidate.monthly_spend_units
                            >= candidate.settings.monthly_top_up_limit_units
                        {
                            continue;
                        }

                        let daily_remaining =
                            candidate.settings.daily_top_up_limit_units
                                - candidate.daily_spend_units;

                        let monthly_remaining =
                            candidate.settings.monthly_top_up_limit_units
                                - candidate.monthly_spend_units;

                        let amount = candidate
                            .settings
                            .auto_top_up_amount_units
                            .min(daily_remaining)
                            .min(monthly_remaining);

                        if amount <= 0 {
                            continue;
                        }

                        let result = self
                            .provider
                            .charge_saved_payment_method(&WalletTopUpRequest {
                                tenant_id: domain::shared::ids::TenantId(candidate.tenant_id),
                                amount_units: amount,
                                currency: "USD".to_owned(),
                                payment_method: candidate.settings.payment_method.clone(),
                            })
                            .await;

                        if matches!(result, Ok(PaymentVerification::Paid)) {
                            let key = format!(
                                "auto-topup:{}:{}",
                                candidate.tenant_id,
                                chrono::Utc::now().date_naive()
                            );

                            let _ = self
                                .wallet
                                .credit(
                                    candidate.tenant_id,
                                    amount,
                                    domain::WalletTransactionType::TopUp,
                                    &key,
                                    Some("Automatic wallet top-up"),
                                )
                                .await;
                        }
                    }
                }
            }
        }
    }
}
