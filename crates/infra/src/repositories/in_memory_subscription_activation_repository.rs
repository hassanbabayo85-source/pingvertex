//! BE-1 F5 — in-memory `SubscriptionActivationRepository` (test-support).
//!
//! WARNING: This adapter is NOT atomic and does NOT provide
//! transactional rollback. It chains calls to the underlying
//! `InMemory*Repository` structs in sequence. Only
//! `PostgresSubscriptionActivationRepository` provides true
//! all-or-nothing semantics via `sqlx::Transaction`.
//!
//! Divergences from Postgres:
//!
//! 1. **Concrete billing repo, not `dyn`.** The trait has no way to
//!    mutate `subscription_id` on an existing record, so this adapter
//!    takes the concrete repo and uses its test-support
//!    `link_subscription_id` method.
//! 2. **No `ON CONFLICT (tenant_id) DO NOTHING`.** `Create` pre-checks
//!    the tenant subscription, then inserts. Not atomic under real
//!    concurrency; Postgres uses `ON CONFLICT`.
//! 3. **No rollback.** Every underlying repo call commits
//!    independently.
//! 4. **Required `provider_payment_id`.** The InMemory `mark_succeeded`
//!    trait takes `&str`, so this adapter errors if the field is `None`
//!    (Postgres binds NULL silently). Theoretical divergence.

use std::sync::Arc;

use application::ports::checkout_session_repository::CheckoutSessionRepository;
use application::ports::subscription_activation_repository::{
    ActivationOutcome, SubscriptionAction, SubscriptionActivationError,
    SubscriptionActivationRepository, SubscriptionActivationResult,
};
use application::ports::subscription_billing_record_repository::{
    BillingRecordStatus, InsertBillingRecordOutcome, SubscriptionBillingRecord,
    SubscriptionBillingRecordRepository,
};
use application::ports::subscription_repository::SubscriptionRepository;
use async_trait::async_trait;
use domain::enums::{CheckoutStatus, SubscriptionStatus};
use domain::shared::ids::CheckoutSessionId;

use super::in_memory_checkout_session_repository::InMemoryCheckoutSessionRepository;
use super::in_memory_subscription_billing_record_repository::InMemorySubscriptionBillingRecordRepository;
use super::in_memory_subscription_repository::InMemorySubscriptionRepository;

#[derive(Clone)]
pub struct InMemorySubscriptionActivationRepository {
    subscriptions: Arc<InMemorySubscriptionRepository>,
    billing_records: Arc<InMemorySubscriptionBillingRecordRepository>,
    checkout_sessions: Arc<InMemoryCheckoutSessionRepository>,
}

impl InMemorySubscriptionActivationRepository {
    pub fn new(
        subscriptions: Arc<InMemorySubscriptionRepository>,
        billing_records: Arc<InMemorySubscriptionBillingRecordRepository>,
        checkout_sessions: Arc<InMemoryCheckoutSessionRepository>,
    ) -> Self {
        Self {
            subscriptions,
            billing_records,
            checkout_sessions,
        }
    }
}

#[async_trait]
impl SubscriptionActivationRepository for InMemorySubscriptionActivationRepository {
    async fn activate_atomically(
        &self,
        record: SubscriptionBillingRecord,
        action: SubscriptionAction,
        session_id: CheckoutSessionId,
    ) -> SubscriptionActivationResult<ActivationOutcome> {
        let provider_payment_id = record
            .provider_payment_id
            .as_deref()
            .ok_or(SubscriptionActivationError::MissingProviderPaymentId)?;

        // ── Step 1 — try insert ────────────────────────────────────
        let outcome = self.billing_records.try_insert(&record).await?;
        let record_was_inserted = outcome == InsertBillingRecordOutcome::Inserted;

        // ── Step 2 — load existing record if not inserted ──────────
        let existing: Option<SubscriptionBillingRecord> = if record_was_inserted {
            None
        } else {
            Some(
                self.billing_records
                    .find_by_provider_reference(&record.provider, &record.provider_reference)
                    .await?
                    .ok_or(SubscriptionActivationError::BillingRecordMissingAfterConflict)?,
            )
        };

        // ── Step 3 — validate identity invariants ──────────────────
        if let Some(existing) = &existing {
            if existing.tenant_id != record.tenant_id {
                return Err(SubscriptionActivationError::BillingRecordTenantMismatch);
            }
            if existing.plan_tier != record.plan_tier {
                return Err(SubscriptionActivationError::BillingRecordPlanTierMismatch);
            }
            if existing.billing_interval != record.billing_interval {
                return Err(SubscriptionActivationError::BillingRecordIntervalMismatch);
            }
            if existing.currency != record.currency {
                return Err(SubscriptionActivationError::BillingRecordCurrencyMismatch);
            }
            if existing.amount_minor != record.amount_minor {
                return Err(SubscriptionActivationError::BillingRecordAmountMismatch);
            }
        }

        // ── Step 3a — Failed → do not reactivate ───────────────────
        if let Some(existing) = &existing
            && existing.status == BillingRecordStatus::Failed
        {
            return Err(SubscriptionActivationError::BillingRecordFailed);
        }

        // ── Step 3b — Succeeded → strengthen validation, then complete ──
        if let Some(existing) = &existing
            && existing.status == BillingRecordStatus::Succeeded
        {
            let subscription_id = existing.subscription_id;
            let sub = self
                .subscriptions
                .find_by_tenant_id(domain::shared::ids::TenantId(record.tenant_id))
                .await?;

            let sub = match sub {
                Some(s) if s.id.0 == subscription_id => s,
                _ => return Err(SubscriptionActivationError::SucceededSubscriptionMissing),
            };

            if sub.status == SubscriptionStatus::Canceled
                || sub.status == SubscriptionStatus::Expired
            {
                return Err(SubscriptionActivationError::SucceededSubscriptionTerminal);
            }

            complete_session(
                &self.checkout_sessions,
                session_id,
                record.tenant_id,
                provider_payment_id,
            )
            .await?;

            return Ok(ActivationOutcome::AlreadySucceeded { subscription_id });
        }

        // ── Step 4 — persist subscription ──────────────────────────
        let (final_subscription_id, outcome) = match action {
            SubscriptionAction::Update(sub) => {
                if sub.status == SubscriptionStatus::Canceled {
                    return Err(SubscriptionActivationError::UpdateOnCanceledSubscription);
                }
                self.subscriptions.update(&sub).await?;
                (
                    sub.id.0,
                    ActivationOutcome::Updated {
                        subscription_id: sub.id.0,
                    },
                )
            }
            SubscriptionAction::Create(sub) => {
                let pre_existing = self.subscriptions.find_by_tenant_id(sub.tenant_id).await?;

                if let Some(winner) = pre_existing {
                    if winner.tenant_id.0 != sub.tenant_id.0 {
                        return Err(SubscriptionActivationError::ConcurrentWinnerTenantMismatch);
                    }
                    if winner.plan_tier != sub.plan_tier {
                        return Err(SubscriptionActivationError::ConcurrentWinnerPlanMismatch);
                    }
                    if winner.status == SubscriptionStatus::Canceled
                        || winner.status == SubscriptionStatus::Expired
                    {
                        return Err(SubscriptionActivationError::ConcurrentWinnerTerminal);
                    }
                    (
                        winner.id.0,
                        ActivationOutcome::ConcurrentWinnerAccepted {
                            subscription_id: winner.id.0,
                        },
                    )
                } else {
                    self.subscriptions.create(&sub).await?;
                    (
                        sub.id.0,
                        ActivationOutcome::Created {
                            subscription_id: sub.id.0,
                        },
                    )
                }
            }
        };

        // ── Step 4b — link record to persisted subscription ────────
        let needs_sync = !record_was_inserted || final_subscription_id != record.subscription_id;
        if needs_sync
            && !self.billing_records.link_subscription_id(
                &record.provider,
                &record.provider_reference,
                final_subscription_id,
            )
        {
            return Err(SubscriptionActivationError::BillingRecordLinkFailed);
        }

        // ── Step 5 — mark billing record succeeded ─────────────────
        let marked = self
            .billing_records
            .mark_succeeded(
                &record.provider,
                &record.provider_reference,
                provider_payment_id,
            )
            .await?;
        if !marked {
            return Err(SubscriptionActivationError::BillingRecordMarkSucceededFailed);
        }

        // ── Step 6 — complete checkout session ─────────────────────
        complete_session(
            &self.checkout_sessions,
            session_id,
            record.tenant_id,
            provider_payment_id,
        )
        .await?;

        Ok(outcome)
    }
}

async fn complete_session(
    checkout_sessions: &Arc<InMemoryCheckoutSessionRepository>,
    session_id: CheckoutSessionId,
    tenant_id: uuid::Uuid,
    provider_payment_id: &str,
) -> SubscriptionActivationResult<()> {
    let session = checkout_sessions
        .find_by_id(session_id)
        .await?
        .ok_or(SubscriptionActivationError::CheckoutSessionNotFound)?;

    if session.tenant_id.0 != tenant_id {
        return Err(SubscriptionActivationError::CheckoutSessionTenantMismatch);
    }
    if session.status == CheckoutStatus::Failed {
        return Err(SubscriptionActivationError::CheckoutSessionAlreadyFailed);
    }

    let mut updated = session;
    updated.mark_completed();
    updated.attach_provider_payment_id(provider_payment_id.to_owned());
    checkout_sessions.update(&updated).await?;

    Ok(())
}
