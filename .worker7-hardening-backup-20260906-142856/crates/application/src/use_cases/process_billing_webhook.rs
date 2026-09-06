use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use domain::shared::ids::{AuditLogEntryId, TenantId};
use domain::{
    BillingAuditLogEntry, BillingWebhookEventType, PlanTier, Subscription, SubscriptionId,
};

use crate::commands::{ProcessBillingWebhookCommand, ProcessBillingWebhookOutcome};
use crate::ports::billing_webhook_idempotency_repository::WebhookIdempotencyClaim;
use crate::ports::{
    AddOnSubscriptionRepository, BillingAuditLogRepository, BillingWebhookIdempotencyRepository,
    SubscriptionRepository, WalletRepository,
};
use crate::use_cases::billing_webhook_support::BillingWebhookError;

const WEBHOOK_STALE_PROCESSING_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct ProcessBillingWebhookUseCase<I, A, S> {
    idempotency_repository: I,
    audit_log_repository: A,
    subscription_repository: S,
    addon_repository: Option<std::sync::Arc<dyn AddOnSubscriptionRepository>>,
    wallet_repository: Option<std::sync::Arc<dyn WalletRepository>>,
}

impl<I, A, S> ProcessBillingWebhookUseCase<I, A, S>
where
    I: BillingWebhookIdempotencyRepository,
    A: BillingAuditLogRepository,
    S: SubscriptionRepository,
{
    pub fn new(
        idempotency_repository: I,
        audit_log_repository: A,
        subscription_repository: S,
    ) -> Self {
        Self {
            idempotency_repository,
            audit_log_repository,
            subscription_repository,
            addon_repository: None,
            wallet_repository: None,
        }
    }

    pub fn with_addon_repository(
        mut self,
        r: std::sync::Arc<dyn AddOnSubscriptionRepository>,
    ) -> Self {
        self.addon_repository = Some(r);
        self
    }
    pub fn with_wallet_repository(mut self, r: std::sync::Arc<dyn WalletRepository>) -> Self {
        self.wallet_repository = Some(r);
        self
    }

    pub async fn execute(
        &self,
        command: ProcessBillingWebhookCommand,
    ) -> Result<ProcessBillingWebhookOutcome, BillingWebhookError> {
        let claim = self
            .idempotency_repository
            .try_claim(&command.provider_event_id, WEBHOOK_STALE_PROCESSING_TIMEOUT)
            .await?;

        match claim {
            WebhookIdempotencyClaim::AlreadyProcessed | WebhookIdempotencyClaim::InProgress => {
                return Ok(ProcessBillingWebhookOutcome { processed: false });
            }
            WebhookIdempotencyClaim::Claimed => {}
        }

        let result = self.process_webhook(&command).await;

        match result {
            Ok(()) => {
                self.idempotency_repository
                    .mark_processed(&command.provider_event_id)
                    .await?;
                Ok(ProcessBillingWebhookOutcome { processed: true })
            }
            Err(err) => {
                let _ = self
                    .idempotency_repository
                    .mark_pending(&command.provider_event_id)
                    .await;
                Err(err)
            }
        }
    }

    async fn process_webhook(
        &self,
        command: &ProcessBillingWebhookCommand,
    ) -> Result<(), BillingWebhookError> {
        let is_wallet_topup = command
            .payload
            .get("wallet_top_up")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_wallet_topup
            && matches!(
                command.event_type,
                BillingWebhookEventType::PaymentSucceeded
            )
        {
            let wallet = self.wallet_repository.as_ref().ok_or_else(|| {
                BillingWebhookError::Other(
                    std::io::Error::other("wallet repository not configured").into(),
                )
            })?;
            let checkout_id = command
                .payload
                .get("provider_checkout_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| BillingWebhookError::Other(std::io::Error::other(
                    "wallet top-up checkout id missing"
                ).into()))?;
            let payment_id = command
                .payload
                .get("payment_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| BillingWebhookError::Other(std::io::Error::other(
                    "wallet top-up payment id missing"
                ).into()))?;

            let session = wallet
                .find_top_up_session(command.tenant_id.0, checkout_id)
                .await
                .map_err(BillingWebhookError::Other)?
                .ok_or_else(|| BillingWebhookError::Other(std::io::Error::other(
                    "wallet top-up session not found"
                ).into()))?;

            if session.status != domain::WalletTopUpSession::PENDING
                && session.status != domain::WalletTopUpSession::COMPLETED
            {
                return Err(BillingWebhookError::Other(std::io::Error::other(
                    "wallet top-up session is not payable"
                ).into()));
            }

            if let Some(payload_amount) = command.payload.get("amount_units").and_then(|v| v.as_i64()) {
                if payload_amount != session.amount_units {
                    return Err(BillingWebhookError::Other(std::io::Error::other(
                        "wallet top-up amount does not match checkout session"
                    ).into()));
                }
            }

            let completed = wallet
                .complete_top_up_session(command.tenant_id.0, checkout_id, payment_id)
                .await
                .map_err(BillingWebhookError::Other)?
                .ok_or_else(|| BillingWebhookError::Other(std::io::Error::other(
                    "failed to complete wallet top-up session"
                ).into()))?;

            wallet
                .credit(
                    command.tenant_id.0,
                    completed.amount_units,
                    domain::WalletTransactionType::TopUp,
                    &format!("topup:{}", completed.provider_payment_id.as_deref().unwrap_or(payment_id)),
                    Some("Wallet top-up"),
                )
                .await
                .map_err(BillingWebhookError::Other)?;
            let _ = wallet
                .record_balance_notification(
                    command.tenant_id.0,
                    "TOP_UP_SUCCESS",
                    &command.provider_event_id,
                    "Wallet top-up successful",
                )
                .await;
        } else if matches!(command.event_type, BillingWebhookEventType::PaymentRefunded) {
            let wallet = self.wallet_repository.as_ref().ok_or_else(|| {
                BillingWebhookError::Other(
                    std::io::Error::other("wallet repository not configured").into(),
                )
            })?;
            let amount = command
                .payload
                .get("amount_units")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| {
                    BillingWebhookError::Other(
                        std::io::Error::other("refund amount missing").into(),
                    )
                })?;
            let payment_id = command
                .payload
                .get("payment_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| BillingWebhookError::Other(std::io::Error::other(
                    "refund payment id missing"
                ).into()))?;
            let refund_id = command
                .payload
                .get("refund_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| BillingWebhookError::Other(std::io::Error::other(
                    "refund id missing"
                ).into()))?;
            let credited = wallet
                .record_verified_refund(
                    command.tenant_id.0,
                    payment_id,
                    amount,
                    refund_id,
                )
                .await
                .map_err(BillingWebhookError::Other)?;
            if !credited {
                return Err(BillingWebhookError::Other(std::io::Error::other(
                    "refund does not match a refundable completed payment"
                ).into()));
            }
            let _ = wallet
                .record_balance_notification(
                    command.tenant_id.0,
                    "REFUND_SUCCESS",
                    &command.provider_event_id,
                    "Refund credited to your wallet",
                )
                .await;
        } else if let Some(id) = command
            .payload
            .get("addon_subscription_id")
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<uuid::Uuid>().ok())
        {
            let repo = self.addon_repository.as_ref().ok_or_else(|| {
                BillingWebhookError::Other(
                    std::io::Error::other("add-on repository not configured").into(),
                )
            })?;
            let mut addon = repo
                .find_by_id(domain::AddOnSubscriptionId(id))
                .await?
                .ok_or(BillingWebhookError::SubscriptionNotFound)?;
            if addon.tenant_id != command.tenant_id {
                return Err(BillingWebhookError::SubscriptionNotFound);
            }
            match command.event_type {
                BillingWebhookEventType::PaymentSucceeded
                | BillingWebhookEventType::SubscriptionRenewed => addon
                    .activate_after_verified_payment()
                    .map_err(|e| BillingWebhookError::Other(std::io::Error::other(e).into()))?,
                BillingWebhookEventType::PaymentFailed => addon
                    .mark_failed()
                    .map_err(|e| BillingWebhookError::Other(std::io::Error::other(e).into()))?,
                BillingWebhookEventType::SubscriptionCanceled => {
                    if addon.status == domain::AddOnStatus::Active {
                        addon.cancel().map_err(|e| {
                            BillingWebhookError::Other(std::io::Error::other(e).into())
                        })?
                    }
                }
                BillingWebhookEventType::PaymentRefunded => {}
            }
            repo.update(&addon).await?;
        } else {
            let subscription = self
                .subscription_repository
                .find_by_tenant_id(command.tenant_id)
                .await?;

            match &command.event_type {
                BillingWebhookEventType::PaymentSucceeded
                | BillingWebhookEventType::SubscriptionRenewed => {
                    self.handle_successful_payment(
                        command.tenant_id,
                        command.plan_tier,
                        subscription,
                    )
                    .await?;
                }
                BillingWebhookEventType::PaymentFailed => {
                    self.handle_failed_payment(command.tenant_id, subscription)
                        .await?;
                }
                BillingWebhookEventType::SubscriptionCanceled => {
                    self.handle_cancellation(
                        command.tenant_id,
                        subscription,
                        command.cancel_at_period_end,
                        command.current_period_ends_at,
                    )
                    .await?;
                }
                BillingWebhookEventType::PaymentRefunded => {}
            }
        }

        let subscription_id = self
            .subscription_repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .map(|s| s.id);

        let audit_entry = BillingAuditLogEntry::new(
            AuditLogEntryId::new(),
            command.provider_event_id.clone(),
            command.event_type,
            command.tenant_id,
            subscription_id,
            command.payload.clone(),
        );
        self.audit_log_repository.create(&audit_entry).await?;

        Ok(())
    }

    async fn handle_successful_payment(
        &self,
        tenant_id: TenantId,
        plan_tier: Option<PlanTier>,
        existing: Option<Subscription>,
    ) -> Result<(), BillingWebhookError> {
        match existing {
            Some(mut subscription) => {
                // The persisted subscription is authoritative for an
                // existing tenant. Never allow webhook payload metadata
                // to overwrite its plan tier.
                subscription
                    .apply_verified_payment(subscription.plan_tier)
                    .map_err(BillingWebhookError::from)?;
                self.subscription_repository.update(&subscription).await?;
            }
            None => {
                let tier = plan_tier.ok_or(BillingWebhookError::MissingPlanTier)?;
                let subscription = Subscription::new(SubscriptionId::new(), tenant_id, tier);
                self.subscription_repository.create(&subscription).await?;
            }
        }
        Ok(())
    }

    async fn handle_failed_payment(
        &self,
        _tenant_id: TenantId,
        existing: Option<Subscription>,
    ) -> Result<(), BillingWebhookError> {
        let mut subscription = existing.ok_or(BillingWebhookError::SubscriptionNotFound)?;
        let grace_ends_at = Utc::now() + ChronoDuration::days(3);
        subscription
            .begin_grace_period(grace_ends_at)
            .map_err(BillingWebhookError::from)?;
        self.subscription_repository.update(&subscription).await?;
        Ok(())
    }

    async fn handle_cancellation(
        &self,
        _tenant_id: TenantId,
        existing: Option<Subscription>,
        at_period_end: bool,
        period_ends_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(), BillingWebhookError> {
        let mut subscription = existing.ok_or(BillingWebhookError::SubscriptionNotFound)?;
        subscription
            .cancel(at_period_end, period_ends_at)
            .map_err(BillingWebhookError::from)?;
        self.subscription_repository.update(&subscription).await?;
        Ok(())
    }
}
