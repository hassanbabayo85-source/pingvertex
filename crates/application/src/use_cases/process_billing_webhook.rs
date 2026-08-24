use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use domain::shared::ids::{AuditLogEntryId, TenantId};
use domain::{
    BillingAuditLogEntry, BillingWebhookEventType, PlanTier, Subscription, SubscriptionId,
};

use crate::commands::{ProcessBillingWebhookCommand, ProcessBillingWebhookOutcome};
use crate::ports::billing_webhook_idempotency_repository::WebhookIdempotencyClaim;
use crate::ports::{
    BillingAuditLogRepository, BillingWebhookIdempotencyRepository, SubscriptionRepository,
};
use crate::use_cases::billing_webhook_support::BillingWebhookError;

const WEBHOOK_STALE_PROCESSING_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct ProcessBillingWebhookUseCase<I, A, S> {
    idempotency_repository: I,
    audit_log_repository: A,
    subscription_repository: S,
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
        }
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
        let subscription = self
            .subscription_repository
            .find_by_tenant_id(command.tenant_id)
            .await?;

        match &command.event_type {
            BillingWebhookEventType::PaymentSucceeded
            | BillingWebhookEventType::SubscriptionRenewed => {
                self.handle_successful_payment(command.tenant_id, command.plan_tier, subscription)
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
                let target_tier = plan_tier.unwrap_or(subscription.plan_tier);
                subscription
                    .apply_verified_payment(target_tier)
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
