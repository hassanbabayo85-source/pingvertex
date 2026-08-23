use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::{PlanTier, SubscriptionStatus};
use crate::errors::DomainError;
use crate::shared::ids::{SubscriptionId, TenantId};

/// A tenant's current plan assignment and lifecycle state.
///
/// B11 modeled only "which plan tier is this tenant on." B12 (this
/// worker) adds the lifecycle on top: trial, grace period after a
/// failed/expired payment, expiry, and cancellation (immediate or at
/// end of period). B14 adds `apply_verified_payment`, which moves an
/// existing subscription back to `Active` after server-side payment
/// verification and updates the plan tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub tenant_id: TenantId,
    pub plan_tier: PlanTier,
    pub status: SubscriptionStatus,
    /// When the trial ends. Only meaningful while `status == Trialing`.
    pub trial_ends_at: Option<DateTime<Utc>>,
    /// When the current grace period ends. Only meaningful while
    /// `status == PastDue`.
    pub grace_period_ends_at: Option<DateTime<Utc>>,
    /// The end of the current billing period. Set when an end-of-period
    /// cancellation is scheduled; used by `finalize_scheduled_cancellation`
    /// once that period actually ends.
    pub current_period_ends_at: Option<DateTime<Utc>>,
    /// When cancellation was requested (immediate or scheduled).
    pub canceled_at: Option<DateTime<Utc>>,
    /// `true` once a cancellation has been requested with
    /// `at_period_end = true` and not yet finalized.
    pub cancel_at_period_end: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Subscription {
    /// Creates a new, immediately-active subscription for a tenant on
    /// the given plan tier (no trial).
    pub fn new(id: SubscriptionId, tenant_id: TenantId, plan_tier: PlanTier) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            plan_tier,
            status: SubscriptionStatus::Active,
            trial_ends_at: None,
            grace_period_ends_at: None,
            current_period_ends_at: None,
            canceled_at: None,
            cancel_at_period_end: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Creates a new subscription that starts in a trial period. The
    /// caller decides the trial length (no fixed duration is baked into
    /// the domain).
    pub fn new_trialing(
        id: SubscriptionId,
        tenant_id: TenantId,
        plan_tier: PlanTier,
        trial_ends_at: DateTime<Utc>,
    ) -> Self {
        let mut subscription = Self::new(id, tenant_id, plan_tier);
        subscription.status = SubscriptionStatus::Trialing;
        subscription.trial_ends_at = Some(trial_ends_at);
        subscription
    }

    /// Applies a server-verified successful payment.
    ///
    /// Moves the subscription to `Active`, updates the plan tier, and
    /// clears every lifecycle marker from a previous trial/grace-period/
    /// cancellation schedule. Rejected only from `Canceled`; use a new
    /// subscription for a tenant that has explicitly canceled.
    pub fn apply_verified_payment(&mut self, plan_tier: PlanTier) -> Result<(), DomainError> {
        if self.status == SubscriptionStatus::Canceled {
            return Err(DomainError::InvalidSubscriptionTransition(
                "cannot apply verified payment to a canceled subscription".to_owned(),
            ));
        }

        self.plan_tier = plan_tier;
        self.status = SubscriptionStatus::Active;
        self.trial_ends_at = None;
        self.grace_period_ends_at = None;
        self.current_period_ends_at = None;
        self.canceled_at = None;
        self.cancel_at_period_end = false;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Moves an `Active` subscription into its grace period after a
    /// failed/expired payment. Not valid from `Trialing` — a trial was
    /// never billed, so there is no payment to have failed.
    pub fn begin_grace_period(
        &mut self,
        grace_period_ends_at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        if self.status != SubscriptionStatus::Active {
            return Err(DomainError::InvalidSubscriptionTransition(format!(
                "cannot begin grace period from status {:?}",
                self.status
            )));
        }
        self.status = SubscriptionStatus::PastDue;
        self.grace_period_ends_at = Some(grace_period_ends_at);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Expires the subscription: reachable from `PastDue` (grace period
    /// ran out) or `Trialing` (trial ran out unconverted). Entitlements
    /// fall back to Free once `Expired` (see `ResolveEntitlementsUseCase`).
    pub fn expire(&mut self) -> Result<(), DomainError> {
        match self.status {
            SubscriptionStatus::PastDue | SubscriptionStatus::Trialing => {
                self.status = SubscriptionStatus::Expired;
                self.grace_period_ends_at = None;
                self.trial_ends_at = None;
                self.updated_at = Utc::now();
                Ok(())
            }
            other => Err(DomainError::InvalidSubscriptionTransition(format!(
                "cannot expire from status {other:?}"
            ))),
        }
    }

    /// Cancels the subscription, either immediately or at the end of
    /// the current billing period. Valid from `Trialing`, `Active`, or
    /// `PastDue` — not from an already `Canceled` or `Expired`
    /// subscription.
    ///
    /// If `at_period_end` is `true`, `current_period_ends_at` must be
    /// provided; the status does not change yet — call
    /// `finalize_scheduled_cancellation` once that period actually ends.
    pub fn cancel(
        &mut self,
        at_period_end: bool,
        current_period_ends_at: Option<DateTime<Utc>>,
    ) -> Result<(), DomainError> {
        if matches!(
            self.status,
            SubscriptionStatus::Canceled | SubscriptionStatus::Expired
        ) {
            return Err(DomainError::InvalidSubscriptionTransition(format!(
                "cannot cancel from status {:?}",
                self.status
            )));
        }

        if at_period_end {
            let period_end =
                current_period_ends_at.ok_or(DomainError::ScheduledCancellationMissingPeriodEnd)?;
            self.cancel_at_period_end = true;
            self.current_period_ends_at = Some(period_end);
            self.canceled_at = Some(Utc::now());
            self.updated_at = Utc::now();
        } else {
            self.status = SubscriptionStatus::Canceled;
            self.cancel_at_period_end = false;
            self.canceled_at = Some(Utc::now());
            self.trial_ends_at = None;
            self.grace_period_ends_at = None;
            self.updated_at = Utc::now();
        }
        Ok(())
    }

    /// Finalizes a previously-scheduled end-of-period cancellation once
    /// that period has actually ended. Only valid when a cancellation
    /// is actually scheduled and the subscription has not, in the
    /// meantime, already expired (e.g. a payment failure led to expiry
    /// before the scheduled cancellation date arrived).
    pub fn finalize_scheduled_cancellation(&mut self) -> Result<(), DomainError> {
        if !self.cancel_at_period_end {
            return Err(DomainError::InvalidSubscriptionTransition(
                "no scheduled cancellation to finalize".to_owned(),
            ));
        }
        if matches!(
            self.status,
            SubscriptionStatus::Canceled | SubscriptionStatus::Expired
        ) {
            return Err(DomainError::InvalidSubscriptionTransition(format!(
                "cannot finalize scheduled cancellation from status {:?}",
                self.status
            )));
        }
        self.status = SubscriptionStatus::Canceled;
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(tier: PlanTier) -> Subscription {
        Subscription::new(SubscriptionId::new(), TenantId::new(), tier)
    }

    fn trialing(tier: PlanTier) -> Subscription {
        Subscription::new_trialing(
            SubscriptionId::new(),
            TenantId::new(),
            tier,
            Utc::now() + chrono::Duration::days(14),
        )
    }

    #[test]
    fn new_subscription_carries_the_given_tier() {
        let subscription = active(PlanTier::Tier5);
        assert_eq!(subscription.plan_tier, PlanTier::Tier5);
        assert_eq!(subscription.status, SubscriptionStatus::Active);
    }

    #[test]
    fn new_subscription_sets_created_and_updated_at_equal() {
        let subscription = active(PlanTier::Free);
        assert_eq!(subscription.created_at, subscription.updated_at);
    }

    #[test]
    fn new_trialing_starts_in_trialing_status_with_given_trial_end() {
        let trial_end = Utc::now() + chrono::Duration::days(30);
        let subscription = Subscription::new_trialing(
            SubscriptionId::new(),
            TenantId::new(),
            PlanTier::Tier1,
            trial_end,
        );
        assert_eq!(subscription.status, SubscriptionStatus::Trialing);
        assert_eq!(subscription.trial_ends_at, Some(trial_end));
    }

    #[test]
    fn begin_grace_period_transitions_active_to_past_due() {
        let mut subscription = active(PlanTier::Tier1);
        let grace_end = Utc::now() + chrono::Duration::days(3);

        subscription.begin_grace_period(grace_end).unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::PastDue);
        assert_eq!(subscription.grace_period_ends_at, Some(grace_end));
    }

    #[test]
    fn begin_grace_period_rejects_transition_from_trialing() {
        let mut subscription = trialing(PlanTier::Tier1);
        let result = subscription.begin_grace_period(Utc::now());
        assert!(result.is_err());
        assert_eq!(subscription.status, SubscriptionStatus::Trialing);
    }

    #[test]
    fn expire_during_grace_period_transitions_past_due_to_expired() {
        let mut subscription = active(PlanTier::Tier5);
        subscription
            .begin_grace_period(Utc::now() + chrono::Duration::days(3))
            .unwrap();

        subscription.expire().unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Expired);
        assert_eq!(subscription.grace_period_ends_at, None);
    }

    #[test]
    fn expire_transitions_unconverted_trial_to_expired() {
        let mut subscription = trialing(PlanTier::Tier1);
        subscription.expire().unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Expired);
        assert_eq!(subscription.trial_ends_at, None);
    }

    #[test]
    fn expire_rejects_transition_directly_from_active() {
        let mut subscription = active(PlanTier::Tier1);
        let result = subscription.expire();
        assert!(result.is_err());
        assert_eq!(subscription.status, SubscriptionStatus::Active);
    }

    #[test]
    fn expire_rejects_transition_from_already_expired() {
        let mut subscription = trialing(PlanTier::Free);
        subscription.expire().unwrap();
        let result = subscription.expire();
        assert!(result.is_err());
    }

    #[test]
    fn cancel_during_trial_transitions_immediately_to_canceled() {
        let mut subscription = trialing(PlanTier::Tier1);
        subscription.cancel(false, None).unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
        assert!(subscription.canceled_at.is_some());
    }

    #[test]
    fn cancel_immediately_transitions_active_to_canceled() {
        let mut subscription = active(PlanTier::Tier10);
        subscription.cancel(false, None).unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
        assert!(!subscription.cancel_at_period_end);
    }

    #[test]
    fn cancel_at_period_end_keeps_status_and_sets_scheduled_flag() {
        let mut subscription = active(PlanTier::Tier5);
        let period_end = Utc::now() + chrono::Duration::days(10);

        subscription.cancel(true, Some(period_end)).unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert!(subscription.cancel_at_period_end);
        assert_eq!(subscription.current_period_ends_at, Some(period_end));
    }

    #[test]
    fn cancel_at_period_end_without_period_end_errors() {
        let mut subscription = active(PlanTier::Tier5);
        let result = subscription.cancel(true, None);
        assert_eq!(
            result,
            Err(DomainError::ScheduledCancellationMissingPeriodEnd)
        );
    }

    #[test]
    fn cancel_rejects_transition_from_already_canceled() {
        let mut subscription = active(PlanTier::Free);
        subscription.cancel(false, None).unwrap();
        let result = subscription.cancel(false, None);
        assert!(result.is_err());
    }

    #[test]
    fn finalize_scheduled_cancellation_transitions_to_canceled() {
        let mut subscription = active(PlanTier::Tier1);
        subscription
            .cancel(true, Some(Utc::now() + chrono::Duration::days(5)))
            .unwrap();

        subscription.finalize_scheduled_cancellation().unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
    }

    #[test]
    fn finalize_scheduled_cancellation_rejects_when_none_scheduled() {
        let mut subscription = active(PlanTier::Tier1);
        let result = subscription.finalize_scheduled_cancellation();
        assert!(result.is_err());
    }

    #[test]
    fn finalize_scheduled_cancellation_rejects_if_already_expired() {
        // A payment failure pushes the subscription into expiry before
        // the scheduled cancellation date ever arrives.
        let mut subscription = active(PlanTier::Tier1);
        subscription
            .cancel(true, Some(Utc::now() + chrono::Duration::days(30)))
            .unwrap();
        subscription
            .begin_grace_period(Utc::now() + chrono::Duration::days(3))
            .unwrap();
        subscription.expire().unwrap();

        let result = subscription.finalize_scheduled_cancellation();
        assert!(result.is_err());
        assert_eq!(subscription.status, SubscriptionStatus::Expired);
    }

    #[test]
    fn apply_verified_payment_activates_expired_subscription_and_updates_tier() {
        let mut subscription = trialing(PlanTier::Free);
        subscription.expire().unwrap();

        subscription
            .apply_verified_payment(PlanTier::Tier5)
            .unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert_eq!(subscription.plan_tier, PlanTier::Tier5);
        assert_eq!(subscription.trial_ends_at, None);
        assert_eq!(subscription.grace_period_ends_at, None);
        assert!(!subscription.cancel_at_period_end);
    }

    #[test]
    fn apply_verified_payment_rejects_canceled_subscription() {
        let mut subscription = active(PlanTier::Tier5);
        subscription.cancel(false, None).unwrap();

        let result = subscription.apply_verified_payment(PlanTier::Tier10);
        assert!(result.is_err());
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
    }
}
