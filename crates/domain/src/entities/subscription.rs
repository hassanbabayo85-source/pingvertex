use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::{PlanTier, SubscriptionStatus};
use crate::errors::DomainError;
use crate::shared::ids::{SubscriptionId, TenantId};

/// A tenant's current plan assignment and lifecycle state.
///
/// Modeled only "which plan tier is this tenant on." (this
/// worker) adds the lifecycle on top: trial, grace period after a
/// failed/expired payment, expiry, and cancellation (immediate or at
/// end of period). adds `apply_verified_payment`, which moves an
/// existing subscription back to `Active` after server-side payment
/// verification and updates the plan tier. adds the current usage
/// snapshot (`monitors`/`interval_seconds`/`retention_days`/
/// `pricing_version`) — the fields that back the `monitors`,
/// `interval_seconds`, `retention_days`, `pricing_version` columns added
/// by migration 0053, which had no domain representation until now.
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
    /// Current usage snapshot. `None` until the tenant's first
    /// checkout/plan-change sets it via `update_usage`. This is a
    /// convenience snapshot of "usage right now"; the authoritative,
    /// billable history lives in `SubscriptionUsagePeriod` rows.
    pub monitors: Option<u64>,
    pub interval_seconds: Option<u32>,
    pub retention_days: Option<u32>,
    pub pricing_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Subscription {
    /// Creates a new, immediately-active subscription for a tenant on
    /// the given plan tier (no trial). Usage fields start unset — call
    /// `update_usage` once the tenant's monitors/interval/retention are
    /// known (e.g. right after checkout).
    pub fn new(id: SubscriptionId, tenant_id: TenantId, plan_tier: PlanTier) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            plan_tier,
            status: SubscriptionStatus::Active,
            trial_ends_at: None,
            grace_period_ends_at: None,
            current_period_ends_at: Some(now + chrono::Duration::days(30)),
            canceled_at: None,
            cancel_at_period_end: false,
            monitors: None,
            interval_seconds: None,
            retention_days: None,
            pricing_version: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Task 6: like `new`, but with a caller-computed period end
    /// (calendar month/year), instead of a hardcoded 30 days.
    pub fn new_with_period(
        id: SubscriptionId,
        tenant_id: TenantId,
        plan_tier: PlanTier,
        current_period_ends_at: DateTime<Utc>,
    ) -> Self {
        let mut s = Self::new(id, tenant_id, plan_tier);
        s.current_period_ends_at = Some(current_period_ends_at);
        s
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
        self.current_period_ends_at = Some(Utc::now() + chrono::Duration::days(30));
        self.canceled_at = None;
        self.cancel_at_period_end = false;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Applies a server-verified successful payment for a calendar
    /// billing period.
    ///
    /// Identical to `apply_verified_payment` except the period end is
    /// supplied by the caller (computed by `domain::billing_period::period_end_from`)
    /// instead of being hardcoded to 30 days. This is the Task 6
    /// subscription-checkout activation path; `apply_verified_payment`
    /// is retained unchanged for legacy callers.
    pub fn apply_verified_payment_with_period(
        &mut self,
        plan_tier: PlanTier,
        period_end: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        if self.status == SubscriptionStatus::Canceled {
            return Err(DomainError::InvalidSubscriptionTransition(
                "cannot apply verified payment to a canceled subscription".to_owned(),
            ));
        }
        if period_end <= Utc::now() {
            return Err(DomainError::InvalidSubscriptionTransition(
                "subscription period end must be in the future".to_owned(),
            ));
        }

        self.plan_tier = plan_tier;
        self.status = SubscriptionStatus::Active;
        self.trial_ends_at = None;
        self.grace_period_ends_at = None;
        self.current_period_ends_at = Some(period_end);
        self.canceled_at = None;
        self.cancel_at_period_end = false;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Task 6 — lazily finalizes the paid period when it has elapsed.
    ///
    /// Because Task 6 uses one-time payments (no native recurring), the
    /// customer's subscription does NOT auto-renew: it either expires
    /// (no renewal) or is canceled at period end (customer requested).
    /// This is safe to call from any read path; it is a no-op unless
    /// the subscription is `Active` and `current_period_ends_at` has
    /// elapsed.
    ///
    /// BE-1 Step 1 — when the transition completes a scheduled
    /// cancellation (`cancel_at_period_end == true`), clear the
    /// `cancel_at_period_end` flag because the cancellation is no
    /// longer pending. `canceled_at` is preserved: it records when the
    /// cancellation was requested (set by `cancel(at_period_end=true,
    /// …)`), not when it took effect. When the transition is a plain
    /// expiry (no scheduled cancellation), `canceled_at` is left
    /// untouched, matching `expire()` and
    /// `finalize_scheduled_cancellation`, which also do not clear it.
    pub fn finalize_period_end(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<Option<SubscriptionStatus>, DomainError> {
        if self.status != SubscriptionStatus::Active {
            return Ok(None);
        }
        match self.current_period_ends_at {
            Some(end) if end <= now => {
                if self.cancel_at_period_end {
                    self.status = SubscriptionStatus::Canceled;
                    // The pending scheduled cancellation is now realized;
                    // clear the flag so a subsequent read does not report
                    // a "cancel at period end" that has already happened.
                    self.cancel_at_period_end = false;
                } else {
                    self.status = SubscriptionStatus::Expired;
                }
                self.updated_at = now;
                Ok(Some(self.status))
            }
            _ => Ok(None),
        }
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

    /// Proration Service. Applies a mid-cycle usage change
    /// (monitors/interval_seconds/retention_days). Does NOT touch
    /// `status`/lifecycle fields — usage and lifecycle are independent
    /// axes of this entity. Only basic positivity invariants are
    /// enforced here; plan-tier-specific entitlement limits (max
    /// monitors for a tier, minimum interval for a tier, etc.) are an
    /// application-layer concern (see `ResolveEntitlementsUseCase`),
    /// since they depend on `PricingConfig`/plan data this entity does
    /// not have access to.
    pub fn update_usage(
        &mut self,
        monitors: u64,
        interval_seconds: u32,
        retention_days: u32,
        pricing_version: u32,
    ) -> Result<(), DomainError> {
        if monitors == 0 {
            return Err(DomainError::SubscriptionUsageMonitorsZero);
        }
        if interval_seconds == 0 {
            return Err(DomainError::SubscriptionUsageIntervalSecondsZero);
        }
        if retention_days == 0 {
            return Err(DomainError::SubscriptionUsageRetentionDaysZero);
        }

        self.monitors = Some(monitors);
        self.interval_seconds = Some(interval_seconds);
        self.retention_days = Some(retention_days);
        self.pricing_version = Some(pricing_version);
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
        let subscription = active(PlanTier::Pro);
        assert_eq!(subscription.plan_tier, PlanTier::Pro);
        assert_eq!(subscription.status, SubscriptionStatus::Active);
    }

    #[test]
    fn new_subscription_sets_created_and_updated_at_equal() {
        let subscription = active(PlanTier::Free);
        assert_eq!(subscription.created_at, subscription.updated_at);
    }

    #[test]
    fn new_subscription_starts_with_no_usage_set() {
        let subscription = active(PlanTier::Starter);
        assert_eq!(subscription.monitors, None);
        assert_eq!(subscription.interval_seconds, None);
        assert_eq!(subscription.retention_days, None);
        assert_eq!(subscription.pricing_version, None);
    }

    #[test]
    fn new_trialing_starts_in_trialing_status_with_given_trial_end() {
        let trial_end = Utc::now() + chrono::Duration::days(30);
        let subscription = Subscription::new_trialing(
            SubscriptionId::new(),
            TenantId::new(),
            PlanTier::Starter,
            trial_end,
        );
        assert_eq!(subscription.status, SubscriptionStatus::Trialing);
        assert_eq!(subscription.trial_ends_at, Some(trial_end));
    }

    #[test]
    fn begin_grace_period_transitions_active_to_past_due() {
        let mut subscription = active(PlanTier::Starter);
        let grace_end = Utc::now() + chrono::Duration::days(3);

        subscription.begin_grace_period(grace_end).unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::PastDue);
        assert_eq!(subscription.grace_period_ends_at, Some(grace_end));
    }

    #[test]
    fn begin_grace_period_rejects_transition_from_trialing() {
        let mut subscription = trialing(PlanTier::Starter);
        let result = subscription.begin_grace_period(Utc::now());
        assert!(result.is_err());
        assert_eq!(subscription.status, SubscriptionStatus::Trialing);
    }

    #[test]
    fn expire_during_grace_period_transitions_past_due_to_expired() {
        let mut subscription = active(PlanTier::Pro);
        subscription
            .begin_grace_period(Utc::now() + chrono::Duration::days(3))
            .unwrap();

        subscription.expire().unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Expired);
        assert_eq!(subscription.grace_period_ends_at, None);
    }

    #[test]
    fn expire_transitions_unconverted_trial_to_expired() {
        let mut subscription = trialing(PlanTier::Starter);
        subscription.expire().unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Expired);
        assert_eq!(subscription.trial_ends_at, None);
    }

    #[test]
    fn expire_rejects_transition_directly_from_active() {
        let mut subscription = active(PlanTier::Starter);
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
        let mut subscription = trialing(PlanTier::Starter);
        subscription.cancel(false, None).unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
        assert!(subscription.canceled_at.is_some());
    }

    #[test]
    fn cancel_immediately_transitions_active_to_canceled() {
        let mut subscription = active(PlanTier::Business);
        subscription.cancel(false, None).unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
        assert!(!subscription.cancel_at_period_end);
    }

    #[test]
    fn cancel_at_period_end_keeps_status_and_sets_scheduled_flag() {
        let mut subscription = active(PlanTier::Pro);
        let period_end = Utc::now() + chrono::Duration::days(10);

        subscription.cancel(true, Some(period_end)).unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert!(subscription.cancel_at_period_end);
        assert_eq!(subscription.current_period_ends_at, Some(period_end));
    }

    #[test]
    fn cancel_at_period_end_without_period_end_errors() {
        let mut subscription = active(PlanTier::Pro);
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
        let mut subscription = active(PlanTier::Starter);
        subscription
            .cancel(true, Some(Utc::now() + chrono::Duration::days(5)))
            .unwrap();

        subscription.finalize_scheduled_cancellation().unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
    }

    #[test]
    fn finalize_scheduled_cancellation_rejects_when_none_scheduled() {
        let mut subscription = active(PlanTier::Starter);
        let result = subscription.finalize_scheduled_cancellation();
        assert!(result.is_err());
    }

    #[test]
    fn finalize_scheduled_cancellation_rejects_if_already_expired() {
        // A payment failure pushes the subscription into expiry before
        // the scheduled cancellation date ever arrives.
        let mut subscription = active(PlanTier::Starter);
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

        subscription.apply_verified_payment(PlanTier::Pro).unwrap();

        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert_eq!(subscription.plan_tier, PlanTier::Pro);
        assert_eq!(subscription.trial_ends_at, None);
        assert_eq!(subscription.grace_period_ends_at, None);
        assert!(!subscription.cancel_at_period_end);
    }

    #[test]
    fn apply_verified_payment_rejects_canceled_subscription() {
        let mut subscription = active(PlanTier::Pro);
        subscription.cancel(false, None).unwrap();

        let result = subscription.apply_verified_payment(PlanTier::Business);
        assert!(result.is_err());
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
    }

    // Proration Service:

    #[test]
    fn update_usage_sets_usage_fields() {
        let mut subscription = active(PlanTier::Pro);
        subscription.update_usage(100, 60, 30, 1).unwrap();
        assert_eq!(subscription.monitors, Some(100));
        assert_eq!(subscription.interval_seconds, Some(60));
        assert_eq!(subscription.retention_days, Some(30));
        assert_eq!(subscription.pricing_version, Some(1));
    }

    #[test]
    fn update_usage_bumps_updated_at() {
        let mut subscription = active(PlanTier::Pro);
        let before = subscription.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        subscription.update_usage(100, 60, 30, 1).unwrap();
        assert!(subscription.updated_at > before);
    }

    #[test]
    fn update_usage_rejects_zero_monitors() {
        let mut subscription = active(PlanTier::Pro);
        let result = subscription.update_usage(0, 60, 30, 1);
        assert_eq!(result, Err(DomainError::SubscriptionUsageMonitorsZero));
        assert_eq!(subscription.monitors, None);
    }

    #[test]
    fn update_usage_rejects_zero_interval_seconds() {
        let mut subscription = active(PlanTier::Pro);
        let result = subscription.update_usage(100, 0, 30, 1);
        assert_eq!(
            result,
            Err(DomainError::SubscriptionUsageIntervalSecondsZero)
        );
    }

    #[test]
    fn update_usage_rejects_zero_retention_days() {
        let mut subscription = active(PlanTier::Pro);
        let result = subscription.update_usage(100, 60, 0, 1);
        assert_eq!(result, Err(DomainError::SubscriptionUsageRetentionDaysZero));
    }

    #[test]
    fn update_usage_can_be_called_again_to_change_usage_mid_cycle() {
        let mut subscription = active(PlanTier::Pro);
        subscription.update_usage(100, 60, 30, 1).unwrap();
        subscription.update_usage(200, 30, 90, 1).unwrap();
        assert_eq!(subscription.monitors, Some(200));
        assert_eq!(subscription.interval_seconds, Some(30));
        assert_eq!(subscription.retention_days, Some(90));
    }

    // ─── BE-1 Step 1 — finalize_period_end state consistency ────────

    #[test]
    fn finalize_period_end_expires_active_when_period_elapsed_without_scheduled_cancel() {
        let mut subscription = active(PlanTier::Pro);
        let past = Utc::now() - chrono::Duration::hours(1);
        subscription.current_period_ends_at = Some(past);
        assert!(!subscription.cancel_at_period_end);
        assert!(subscription.canceled_at.is_none());

        let result = subscription.finalize_period_end(Utc::now()).unwrap();

        assert_eq!(result, Some(SubscriptionStatus::Expired));
        assert_eq!(subscription.status, SubscriptionStatus::Expired);
        assert!(!subscription.cancel_at_period_end);
        assert!(subscription.canceled_at.is_none());
    }

    #[test]
    fn finalize_period_end_cancels_scheduled_when_period_elapsed_and_clears_flag() {
        let mut subscription = active(PlanTier::Pro);
        let period_end = Utc::now() - chrono::Duration::hours(1);
        subscription.cancel(true, Some(period_end)).unwrap();
        assert!(subscription.cancel_at_period_end);
        let canceled_at_before = subscription.canceled_at;
        assert!(
            canceled_at_before.is_some(),
            "cancel(at_period_end=true) must set canceled_at to the request time"
        );

        let result = subscription.finalize_period_end(Utc::now()).unwrap();

        assert_eq!(result, Some(SubscriptionStatus::Canceled));
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
        assert!(
            !subscription.cancel_at_period_end,
            "the scheduled cancellation was realized — cancel_at_period_end must be cleared"
        );
        assert_eq!(
            subscription.canceled_at, canceled_at_before,
            "canceled_at records the request time and must be preserved"
        );
    }

    #[test]
    fn finalize_period_end_is_noop_when_period_not_elapsed() {
        let mut subscription = active(PlanTier::Pro);
        let future = Utc::now() + chrono::Duration::days(15);
        subscription.current_period_ends_at = Some(future);
        let status_before = subscription.status;
        let cancel_at_period_end_before = subscription.cancel_at_period_end;
        let canceled_at_before = subscription.canceled_at;

        let result = subscription.finalize_period_end(Utc::now()).unwrap();

        assert_eq!(result, None);
        assert_eq!(subscription.status, status_before);
        assert_eq!(
            subscription.cancel_at_period_end,
            cancel_at_period_end_before
        );
        assert_eq!(subscription.canceled_at, canceled_at_before);
    }

    #[test]
    fn finalize_period_end_is_idempotent_after_transition() {
        let mut subscription = active(PlanTier::Pro);
        let period_end = Utc::now() - chrono::Duration::hours(1);
        subscription.cancel(true, Some(period_end)).unwrap();
        let first = subscription.finalize_period_end(Utc::now()).unwrap();
        assert_eq!(first, Some(SubscriptionStatus::Canceled));

        let second = subscription.finalize_period_end(Utc::now()).unwrap();
        assert_eq!(
            second, None,
            "second call on a non-Active subscription is a no-op"
        );
        assert_eq!(subscription.status, SubscriptionStatus::Canceled);
        assert!(!subscription.cancel_at_period_end);
    }

    #[test]
    fn finalize_period_end_is_noop_for_past_due_and_trialing() {
        let mut past_due = active(PlanTier::Pro);
        past_due
            .begin_grace_period(Utc::now() + chrono::Duration::days(3))
            .unwrap();
        past_due.current_period_ends_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert_eq!(past_due.finalize_period_end(Utc::now()).unwrap(), None);
        assert_eq!(past_due.status, SubscriptionStatus::PastDue);

        let mut trialing = trialing(PlanTier::Starter);
        trialing.current_period_ends_at = Some(Utc::now() - chrono::Duration::hours(1));
        assert_eq!(trialing.finalize_period_end(Utc::now()).unwrap(), None);
        assert_eq!(trialing.status, SubscriptionStatus::Trialing);
    }

    #[test]
    fn finalize_period_end_is_noop_when_no_period_end_set() {
        let mut subscription = active(PlanTier::Pro);
        subscription.current_period_ends_at = None;

        let result = subscription.finalize_period_end(Utc::now()).unwrap();

        assert_eq!(result, None);
        assert_eq!(subscription.status, SubscriptionStatus::Active);
    }
}
