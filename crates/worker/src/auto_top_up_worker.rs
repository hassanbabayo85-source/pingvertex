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
    financial_audit: Option<Arc<dyn FinancialAuditLogRepository>>,
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
            financial_audit: None,
        }
    }

    pub fn with_financial_audit_repository(
        mut self,
        repository: Arc<dyn FinancialAuditLogRepository>,
    ) -> Self {
        self.financial_audit = Some(repository);
        self
    }

    async fn audit(
        &self,
        tenant_id: uuid::Uuid,
        action: &str,
        target: &str,
        metadata: serde_json::Value,
    ) {
        if let Some(repo) = &self.financial_audit {
            let _ = repo
                .create(&domain::FinancialAuditLogEntry::new(
                    "system:auto-top-up",
                    Some(tenant_id),
                    action,
                    "auto_top_up",
                    Some(target.to_owned()),
                    metadata,
                ))
                .await;
        }
    }

    async fn reconcile(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for attempt in self
            .wallet
            .list_auto_top_up_reconciliation_attempts(50)
            .await?
        {
            let Some(reference) = attempt.provider_reference.as_deref() else {
                continue;
            };
            metrics::counter!("payg_auto_top_up_reconciliation_attempts_total").increment(1);
            match self.provider.verify_payment(reference).await? {
                PaymentVerification::Paid {
                    provider_payment_id,
                    provider_reference,
                    amount_units,
                    currency,
                } => {
                    if amount_units != attempt.amount_units || !currency.eq_ignore_ascii_case("USD")
                    {
                        metrics::counter!("payg_auto_top_up_reconciliation_failure_total")
                            .increment(1);
                        self.wallet
                            .record_auto_top_up_attempt(&AutoTopUpAttempt {
                                id: attempt.id,
                                tenant_id: attempt.tenant_id,
                                amount_units: attempt.amount_units,
                                status: "REQUIRES_RECONCILIATION".into(),
                                provider_payment_id: Some(provider_payment_id),
                                provider_reference: Some(provider_reference),
                                error_message: Some("provider amount/currency mismatch".into()),
                                attempted_at: Utc::now(),
                            })
                            .await?;
                        continue;
                    }
                    let key = format!("auto-topup-provider:{provider_payment_id}");
                    match self
                        .wallet
                        .credit(
                            attempt.tenant_id,
                            amount_units,
                            domain::WalletTransactionType::TopUp,
                            &key,
                            Some("Automatic wallet top-up reconciliation"),
                        )
                        .await
                    {
                        Ok(_) => {
                            metrics::counter!("payg_auto_top_up_reconciliation_success_total")
                                .increment(1);
                            self.wallet
                                .record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt.id,
                                    tenant_id: attempt.tenant_id,
                                    amount_units: attempt.amount_units,
                                    status: "SUCCEEDED".into(),
                                    provider_payment_id: Some(provider_payment_id),
                                    provider_reference: Some(provider_reference),
                                    error_message: None,
                                    attempted_at: Utc::now(),
                                })
                                .await?;
                        }
                        Err(e) => {
                            metrics::counter!("payg_auto_top_up_reconciliation_failure_total")
                                .increment(1);
                            self.wallet
                                .record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt.id,
                                    tenant_id: attempt.tenant_id,
                                    amount_units: attempt.amount_units,
                                    status: "REQUIRES_RECONCILIATION".into(),
                                    provider_payment_id: attempt.provider_payment_id,
                                    provider_reference: attempt.provider_reference,
                                    error_message: Some(format!("wallet credit failed: {e}")),
                                    attempted_at: Utc::now(),
                                })
                                .await?;
                        }
                    }
                }
                PaymentVerification::Pending {
                    provider_payment_id,
                    provider_reference,
                    ..
                } => {
                    metrics::counter!("payg_auto_top_up_pending_total").increment(1);
                    self.wallet
                        .record_auto_top_up_attempt(&AutoTopUpAttempt {
                            id: attempt.id,
                            tenant_id: attempt.tenant_id,
                            amount_units: attempt.amount_units,
                            status: "REQUIRES_RECONCILIATION".into(),
                            provider_payment_id: provider_payment_id
                                .or(attempt.provider_payment_id),
                            provider_reference: provider_reference.or(attempt.provider_reference),
                            error_message: Some("provider payment is still pending".into()),
                            attempted_at: Utc::now(),
                        })
                        .await?;
                }
                PaymentVerification::Failed {
                    provider_payment_id,
                    provider_reference,
                    ..
                } => {
                    metrics::counter!("payg_auto_top_up_failed_total").increment(1);
                    self.wallet
                        .record_auto_top_up_attempt(&AutoTopUpAttempt {
                            id: attempt.id,
                            tenant_id: attempt.tenant_id,
                            amount_units: attempt.amount_units,
                            status: "FAILED".into(),
                            provider_payment_id: provider_payment_id
                                .or(attempt.provider_payment_id),
                            provider_reference: provider_reference.or(attempt.provider_reference),
                            error_message: Some("provider reports payment failed".into()),
                            attempted_at: Utc::now(),
                        })
                        .await?;
                }
            }
        }
        Ok(())
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
                    self.reconcile().await?;
                    for candidate in self.wallet.auto_top_up_candidates(50).await? {
                        if candidate.daily_attempts >= candidate.settings.retry_limit { continue; }
                        let amount = candidate.settings.auto_top_up_amount_units
                            .min(candidate.settings.daily_top_up_limit_units.saturating_sub(candidate.daily_spend_units))
                            .min(candidate.settings.monthly_top_up_limit_units.saturating_sub(candidate.monthly_spend_units));
                        if amount <= 0 { continue; }
                        metrics::counter!("payg_auto_top_up_attempts_total").increment(1);
                        let lease_id = uuid::Uuid::new_v4();
                        if !self.wallet.claim_auto_top_up(candidate.tenant_id, lease_id, Duration::from_secs(600)).await? { continue; }
                        let attempt_id = uuid::Uuid::new_v4();
                        let reference = format!("pv-auto-{}", attempt_id.simple());
                        self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                            id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "PENDING".into(),
                            provider_payment_id: None, provider_reference: Some(reference.clone()), error_message: None, attempted_at: Utc::now(),
                        }).await?;
                        self.audit(candidate.tenant_id, "auto_top_up_attempt", &attempt_id.to_string(), serde_json::json!({"amount_units": amount, "currency": candidate.currency.clone()})).await;
                        let result = self.provider.charge_saved_payment_method(&WalletTopUpRequest {
                            tenant_id: domain::shared::ids::TenantId(candidate.tenant_id), amount_units: amount,
                            currency: candidate.currency.clone(), idempotency_key: reference.clone(),
                            payment_method: candidate.settings.payment_method.clone(), customer_email: candidate.customer_email.clone(),
                        }).await;
                        match result {
                            Ok(PaymentVerification::Paid { provider_payment_id, provider_reference, amount_units, currency }) => {
                                if amount_units != amount || !currency.eq_ignore_ascii_case(&candidate.currency) {
                                    self.audit(candidate.tenant_id, "provider_mismatch", &attempt_id.to_string(), serde_json::json!({"expected_amount_units": amount, "actual_amount_units": amount_units, "expected_currency": candidate.currency.clone(), "actual_currency": currency.clone()})).await;
                                    metrics::counter!("payg_auto_top_up_reconciliation_required_total").increment(1);
                                    self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                        id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "REQUIRES_RECONCILIATION".into(),
                                        provider_payment_id: Some(provider_payment_id), provider_reference: Some(provider_reference),
                                        error_message: Some("provider amount/currency mismatch".into()), attempted_at: Utc::now(),
                                    }).await?;
                                } else {
                                    let key = format!("auto-topup-provider:{provider_payment_id}");
                                    match self.wallet.credit(candidate.tenant_id, amount_units, domain::WalletTransactionType::TopUp, &key, Some("Automatic wallet top-up")).await {
                                        Ok(_) => {
                                            metrics::counter!("payg_auto_top_up_paid_total").increment(1);
                                            self.audit(candidate.tenant_id, "auto_top_up_success", &provider_payment_id, serde_json::json!({"amount_units": amount_units, "currency": currency.clone()})).await;
                                            self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                                id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "SUCCEEDED".into(),
                                                provider_payment_id: Some(provider_payment_id), provider_reference: Some(provider_reference), error_message: None, attempted_at: Utc::now(),
                                            }).await?;
                                        }
                                        Err(e) => {
                                            metrics::counter!("payg_auto_top_up_reconciliation_required_total").increment(1);
                                            self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                                id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "REQUIRES_RECONCILIATION".into(),
                                                provider_payment_id: Some(provider_payment_id), provider_reference: Some(provider_reference),
                                                error_message: Some(format!("wallet credit failed: {e}")), attempted_at: Utc::now(),
                                            }).await?;
                                        }
                                    }
                                }
                            }
                            Ok(PaymentVerification::Pending { provider_payment_id, provider_reference, .. }) => {
                                metrics::counter!("payg_auto_top_up_pending_total").increment(1);
                                metrics::counter!("payg_auto_top_up_reconciliation_required_total").increment(1);
                                self.audit(candidate.tenant_id, "auto_top_up_reconciliation_required", &attempt_id.to_string(), serde_json::json!({"error": "provider_error"})).await;
                                self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "REQUIRES_RECONCILIATION".into(),
                                    provider_payment_id, provider_reference: provider_reference.or(Some(reference)),
                                    error_message: Some("provider payment is pending".into()), attempted_at: Utc::now(),
                                }).await?;
                            }
                            Ok(PaymentVerification::Failed { provider_payment_id, provider_reference, .. }) => {
                                metrics::counter!("payg_auto_top_up_failed_total").increment(1);
                                self.audit(candidate.tenant_id, "auto_top_up_failure", &attempt_id.to_string(), serde_json::json!({"error": "provider_failed"})).await;
                                self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "FAILED".into(),
                                    provider_payment_id, provider_reference: provider_reference.or(Some(reference)),
                                    error_message: Some("automatic top-up payment failed".into()), attempted_at: Utc::now(),
                                }).await?;
                            }
                            Err(e) => {
                                metrics::counter!("payg_auto_top_up_reconciliation_required_total").increment(1);
                                self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt_id, tenant_id: candidate.tenant_id, amount_units: amount, status: "REQUIRES_RECONCILIATION".into(),
                                    provider_payment_id: None, provider_reference: Some(reference),
                                    error_message: Some(format!("provider error; reconciliation required: {e}")), attempted_at: Utc::now(),
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
