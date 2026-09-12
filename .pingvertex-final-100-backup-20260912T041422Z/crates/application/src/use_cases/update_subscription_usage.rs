use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::entities::{Plan, SubscriptionUsagePeriod};
use domain::enums::PlanTier;
use domain::shared::ids::TenantId;

use crate::ports::{
    IdGenerator, SubscriptionRepository, SubscriptionUsagePeriodRepository,
    SubscriptionUsageTransaction,
};

/// Command for H12 — Proration Service: recomputes a tenant's usage
/// snapshot mid-billing-cycle.
///
/// SECURITY (P0): this command intentionally carries NO plan tier or
/// usage numbers. `monitors`/`interval_seconds`/`retention_days` are
/// always derived, inside `execute`, from the tenant's OWN
/// `Subscription::plan_tier` as already stored — never from caller
/// input. A previous version of this command accepted
/// monitors/interval_seconds/retention_days/plan_tier directly from the
/// HTTP request, which let a caller holding `ManageBilling` overwrite
/// their tenant's billing usage with arbitrary (e.g. cheaper) values,
/// independent of what they actually pay for.
#[derive(Debug, Clone)]
pub struct UpdateSubscriptionUsageCommand {
    pub tenant_id: TenantId,
    pub pricing_version: u32,
}

/// Result of a successful `UpdateSubscriptionUsageUseCase::execute` —
/// the usage values that were actually applied, all derived from the
/// subscription's own plan tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSubscriptionUsageOutcome {
    pub plan_tier: PlanTier,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub pricing_version: u32,
}

#[derive(Debug)]
pub enum UpdateSubscriptionUsageError {
    SubscriptionNotFound,
    InvalidUsage(domain::DomainError),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for UpdateSubscriptionUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubscriptionNotFound => write!(f, "no subscription found for this tenant"),
            Self::InvalidUsage(err) => write!(f, "{err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UpdateSubscriptionUsageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SubscriptionNotFound => None,
            Self::InvalidUsage(_) => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

impl From<domain::DomainError> for UpdateSubscriptionUsageError {
    fn from(err: domain::DomainError) -> Self {
        Self::InvalidUsage(err)
    }
}

impl From<Box<dyn Error + Send + Sync>> for UpdateSubscriptionUsageError {
    fn from(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::Other(err)
    }
}

/// H12 — Proration Service.
///
/// When a tenant's usage needs recomputing mid-cycle, applying the new
/// config across the WHOLE billing month (or the OLD config across the
/// whole month) would over- or under-charge them. This use case
/// instead: closes whatever `SubscriptionUsagePeriod` is currently
/// open, opens a new one with the new usage, and updates the
/// `subscriptions` row's usage snapshot — all atomically via
/// `SubscriptionUsageTransaction`. Billing later sums
/// `calculate_period_cost` across every period touched in a cycle
/// (`list_periods_for_cycle`, see `CalculateSubscriptionCycleCostUseCase`)
/// instead of charging a single flat rate for the whole month.
///
/// SECURITY (P0 — fixed): `monitors`/`interval_seconds`/
/// `retention_days` are computed here from `Plan::for_tier(subscription
/// .plan_tier)` — i.e. the tenant's OWN, already-persisted plan
/// assignment — and are NOT accepted from
/// `UpdateSubscriptionUsageCommand`. The subscription's plan tier can
/// only change through the dedicated plan-change/checkout flow, never
/// through this use case. This closes a business-logic authorization
/// gap where a caller holding `ManageBilling` could otherwise select an
/// arbitrary plan tier here and have the billing engine charge for it
/// instead of the tenant's real, paid-for plan.
pub struct UpdateSubscriptionUsageUseCase<T> {
    subscription_repository: Arc<dyn SubscriptionRepository>,
    usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
    id_generator: Arc<dyn IdGenerator>,
    transaction: T,
}

impl<T> UpdateSubscriptionUsageUseCase<T>
where
    T: SubscriptionUsageTransaction,
{
    pub fn new(
        subscription_repository: Arc<dyn SubscriptionRepository>,
        usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
        id_generator: Arc<dyn IdGenerator>,
        transaction: T,
    ) -> Self {
        Self {
            subscription_repository,
            usage_period_repository,
            id_generator,
            transaction,
        }
    }

    pub async fn execute(
        &self,
        command: UpdateSubscriptionUsageCommand,
    ) -> Result<UpdateSubscriptionUsageOutcome, UpdateSubscriptionUsageError> {
        let mut subscription = self
            .subscription_repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .ok_or(UpdateSubscriptionUsageError::SubscriptionNotFound)?;

        // SECURITY (P0): authoritative plan comes from the subscription
        // itself, never from `command`.
        let plan = Plan::for_tier(subscription.plan_tier);

        // Domain validation (monitors/interval/retention > 0) happens
        // here, BEFORE anything is persisted.
        subscription.update_usage(
            plan.max_monitors as u64,
            plan.min_check_interval_seconds,
            plan.history_retention_days,
            command.pricing_version,
        )?;

        let now = Utc::now();

        let previous_open_period = self
            .usage_period_repository
            .find_open_period(subscription.id)
            .await?;

        let new_period_id = self.id_generator.next_subscription_usage_period_id().await;
        let new_period = SubscriptionUsagePeriod::open(
            new_period_id,
            subscription.id,
            plan.max_monitors as u64,
            plan.min_check_interval_seconds,
            plan.history_retention_days,
            now,
        );

        self.transaction
            .change_usage(
                &subscription,
                previous_open_period.map(|p| p.id),
                &new_period,
                None,
            )
            .await?;

        Ok(UpdateSubscriptionUsageOutcome {
            plan_tier: subscription.plan_tier,
            monitors: plan.max_monitors as u64,
            interval_seconds: plan.min_check_interval_seconds,
            retention_days: plan.history_retention_days,
            pricing_version: command.pricing_version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Subscription;
    use domain::shared::ids::SubscriptionId;
    use std::sync::Arc;

    struct FixedSubscriptionRepository {
        subscription: Subscription,
    }

    #[async_trait::async_trait]
    impl SubscriptionRepository for FixedSubscriptionRepository {
        async fn create(
            &self,
            _subscription: &Subscription,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }

        async fn find_by_tenant_id(
            &self,
            _tenant_id: TenantId,
        ) -> Result<Option<Subscription>, Box<dyn Error + Send + Sync>> {
            Ok(Some(self.subscription.clone()))
        }

        async fn update(
            &self,
            _subscription: &Subscription,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
    }

    struct EmptyUsagePeriodRepository;

    #[async_trait::async_trait]
    impl SubscriptionUsagePeriodRepository for EmptyUsagePeriodRepository {
        async fn create_period(
            &self,
            _period: &SubscriptionUsagePeriod,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }

        async fn close_period(
            &self,
            _period_id: domain::shared::ids::SubscriptionUsagePeriodId,
            _end_at: chrono::DateTime<Utc>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }

        async fn find_open_period(
            &self,
            _subscription_id: SubscriptionId,
        ) -> Result<Option<SubscriptionUsagePeriod>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn list_periods_for_cycle(
            &self,
            _subscription_id: SubscriptionId,
            _cycle_start: chrono::DateTime<Utc>,
            _cycle_end: chrono::DateTime<Utc>,
        ) -> Result<Vec<SubscriptionUsagePeriod>, Box<dyn Error + Send + Sync>> {
            Ok(vec![])
        }
    }

    struct NoopIdGenerator;

    #[async_trait::async_trait]
    impl IdGenerator for NoopIdGenerator {
        async fn next_user_id(&self) -> domain::UserId {
            domain::UserId::new()
        }
        async fn next_monitor_id(&self) -> domain::MonitorId {
            domain::MonitorId::new()
        }
        async fn next_check_id(&self) -> domain::CheckId {
            domain::CheckId::new()
        }
        async fn next_incident_id(&self) -> domain::IncidentId {
            domain::IncidentId::new()
        }
        async fn next_notification_channel_id(&self) -> domain::NotificationChannelId {
            domain::NotificationChannelId::new()
        }
        async fn next_api_flow_id(&self) -> domain::ApiFlowId {
            domain::ApiFlowId::new()
        }
        async fn next_webhook_signing_secret(&self) -> String {
            "secret".to_string()
        }
        async fn next_api_key_secret(&self) -> String {
            "secret".to_string()
        }
        async fn next_backup_code(&self) -> String {
            "0000-0000".to_string()
        }
        async fn next_pending_two_factor_token(&self) -> String {
            "token".to_string()
        }
        async fn next_email_verification_token(&self) -> String {
            "evt_token".to_string()
        }
        async fn next_reset_password_token(&self) -> String {
            "rst_token".to_string()
        }
        async fn next_invitation_token(&self) -> String {
            "inv_token".to_string()
        }

        async fn next_email_verification_otp(&self) -> String {
            "000000".to_owned()
        }
    }

    struct AcceptingTransaction;

    #[async_trait::async_trait]
    impl SubscriptionUsageTransaction for AcceptingTransaction {
        async fn change_usage(
            &self,
            _subscription: &Subscription,
            _previous_open_period_id: Option<domain::shared::ids::SubscriptionUsagePeriodId>,
            _new_period: &SubscriptionUsagePeriod,
            _expected_old_plan: Option<domain::enums::PlanTier>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
    }

    /// SECURITY (P0) regression test: the usage applied must come from
    /// the subscription's OWN plan tier (Tier5 here), even though the
    /// command itself carries no plan/usage fields at all for a caller
    /// to tamper with in the first place.
    #[tokio::test]
    async fn execute_derives_usage_only_from_the_subscriptions_own_plan_tier() {
        let subscription = Subscription::new(SubscriptionId::new(), TenantId::new(), PlanTier::Pro);
        let tenant_id = subscription.tenant_id;
        let expected_plan = Plan::for_tier(PlanTier::Pro);

        let use_case = UpdateSubscriptionUsageUseCase::new(
            Arc::new(FixedSubscriptionRepository { subscription }),
            Arc::new(EmptyUsagePeriodRepository),
            Arc::new(NoopIdGenerator),
            AcceptingTransaction,
        );

        let outcome = use_case
            .execute(UpdateSubscriptionUsageCommand {
                tenant_id,
                pricing_version: 3,
            })
            .await
            .expect("usage update should succeed for an active subscription");

        assert_eq!(outcome.plan_tier, PlanTier::Pro);
        assert_eq!(outcome.monitors, expected_plan.max_monitors as u64);
        assert_eq!(
            outcome.interval_seconds,
            expected_plan.min_check_interval_seconds
        );
        assert_eq!(outcome.retention_days, expected_plan.history_retention_days);
        assert_eq!(outcome.pricing_version, 3);
    }

    #[tokio::test]
    async fn execute_fails_when_the_tenant_has_no_subscription() {
        struct EmptySubscriptionRepository;

        #[async_trait::async_trait]
        impl SubscriptionRepository for EmptySubscriptionRepository {
            async fn create(
                &self,
                _subscription: &Subscription,
            ) -> Result<(), Box<dyn Error + Send + Sync>> {
                Ok(())
            }
            async fn find_by_tenant_id(
                &self,
                _tenant_id: TenantId,
            ) -> Result<Option<Subscription>, Box<dyn Error + Send + Sync>> {
                Ok(None)
            }
            async fn update(
                &self,
                _subscription: &Subscription,
            ) -> Result<(), Box<dyn Error + Send + Sync>> {
                Ok(())
            }
        }

        let use_case = UpdateSubscriptionUsageUseCase::new(
            Arc::new(EmptySubscriptionRepository),
            Arc::new(EmptyUsagePeriodRepository),
            Arc::new(NoopIdGenerator),
            AcceptingTransaction,
        );

        let result = use_case
            .execute(UpdateSubscriptionUsageCommand {
                tenant_id: TenantId::new(),
                pricing_version: 1,
            })
            .await;

        assert!(matches!(
            result,
            Err(UpdateSubscriptionUsageError::SubscriptionNotFound)
        ));
    }
}
