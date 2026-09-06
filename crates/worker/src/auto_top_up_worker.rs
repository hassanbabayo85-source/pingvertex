use application::ports::{
    FinancialAuditLogRepository, PaymentProvider, PaymentVerification, WalletRepository,
    WalletTopUpRequest,
};
use chrono::Utc;
use domain::AutoTopUpAttempt;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub struct AutoTopUpWorker {
    wallet: Arc<dyn WalletRepository>,
    provider: Arc<dyn PaymentProvider>,
    interval: Duration,
    audit: Option<Arc<dyn FinancialAuditLogRepository>>,
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
            audit: None,
        }
    }

    pub fn with_audit_repository(mut self, audit: Arc<dyn FinancialAuditLogRepository>) -> Self {
        self.audit = Some(audit);
        self
    }

    pub async fn run_forever(
        &self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = ticker.tick() => {
                    let candidates = self.wallet.auto_top_up_candidates(50).await?;
                    for candidate in candidates {
                        if candidate.daily_attempts >= candidate.settings.retry_limit {
                            continue;
                        }

                        let daily_remaining = candidate.settings.daily_top_up_limit_units
                            .saturating_sub(candidate.daily_spend_units);
                        let monthly_remaining = candidate.settings.monthly_top_up_limit_units
                            .saturating_sub(candidate.monthly_spend_units);
                        let amount = candidate.settings.auto_top_up_amount_units
                            .min(daily_remaining)
                            .min(monthly_remaining);
                        if amount <= 0 { continue; }

                        let lease_id = uuid::Uuid::new_v4();
                        if !self.wallet.claim_auto_top_up(
                            candidate.tenant_id,
                            lease_id,
                            Duration::from_secs(600),
                        ).await? {
                            continue;
                        }

                        let attempt_id = uuid::Uuid::new_v4();
                        let result = self.provider.charge_saved_payment_method(&WalletTopUpRequest {
                            tenant_id: domain::shared::ids::TenantId(candidate.tenant_id),
                            amount_units: amount,
                            currency: "USD".to_owned(),
                            idempotency_key: format!("auto-topup:{attempt_id}"),
                            payment_method: candidate.settings.payment_method.clone(),
                        }).await;

                        match result {
                            Ok(PaymentVerification::Paid) => {
                                self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt_id,
                                    tenant_id: candidate.tenant_id,
                                    amount_units: amount,
                                    status: "SUCCEEDED".to_owned(),
                                    provider_reference: None,
                                    error_message: None,
                                    attempted_at: Utc::now(),
                                }).await?;
                                let key = format!("auto-topup:{attempt_id}");
                                self.wallet.credit(
                                    candidate.tenant_id, amount,
                                    domain::WalletTransactionType::TopUp,
                                    &key,
                                    Some("Automatic wallet top-up"),
                                ).await?;
                                if let Some(audit)=&self.audit { audit.create(&domain::FinancialAuditLogEntry::new("system:auto_top_up",Some(candidate.tenant_id),"wallet_credited","wallet",None,serde_json::json!({"amount_units":amount,"idempotency_key":key}))).await?; }
                                let _ = self.wallet.record_balance_notification(
                                    candidate.tenant_id,
                                    "TOP_UP_SUCCESS",
                                    &attempt_id.to_string(),
                                    "Automatic wallet top-up successful",
                                ).await;
                            }
                            Ok(PaymentVerification::Pending) => {
                                self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt_id,
                                    tenant_id: candidate.tenant_id,
                                    amount_units: amount,
                                    status: "PENDING".to_owned(),
                                    provider_reference: None,
                                    error_message: Some("payment provider returned pending".to_owned()),
                                    attempted_at: Utc::now(),
                                }).await?;
                            }
                            Ok(PaymentVerification::Failed) | Err(_) => {
                                self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt_id,
                                    tenant_id: candidate.tenant_id,
                                    amount_units: amount,
                                    status: "FAILED".to_owned(),
                                    provider_reference: None,
                                    error_message: Some("automatic top-up payment failed".to_owned()),
                                    attempted_at: Utc::now(),
                                }).await?;
                            }
                        }
                        let _ = self.wallet.release_auto_top_up(candidate.tenant_id, lease_id).await;
                    }
                }
            }
        }
    }
}
