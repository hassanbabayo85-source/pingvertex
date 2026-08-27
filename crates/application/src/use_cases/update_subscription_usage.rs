use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::entities::SubscriptionUsagePeriod;
use domain::shared::ids::TenantId;

use crate::ports::{
    IdGenerator, SubscriptionRepository, SubscriptionUsagePeriodRepository,
    SubscriptionUsageTransaction,
};

/// Command for H12 — Proration Service: a tenant changes monitors,
/// interval, and/or retention mid-billing-cycle.
#[derive(Debug, Clone)]
pub struct UpdateSubscriptionUsageCommand {
    pub tenant_id: TenantId,
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
/// When a tenant changes monitors/interval_seconds/retention_days
/// mid-cycle, applying the new config across the WHOLE billing month (or
/// the OLD config across the whole month) would over- or under-charge
/// them. This use case instead: closes whatever `SubscriptionUsagePeriod`
/// is currently open, opens a new one with the new usage, and updates the
/// `subscriptions` row's usage snapshot — all atomically via
/// `SubscriptionUsageTransaction`. Billing later sums
/// `calculate_period_cost` across every period touched in a cycle
/// (`list_periods_for_cycle`, see `CalculateSubscriptionCycleCostUseCase`)
/// instead of charging a single flat rate for the whole month.
///
/// NOTE: this is a new application-layer entry point — at the time this
/// worker ran, no prior use case mutated a subscription's usage fields
/// (they were added by migration 0053 but never wired to any domain/
/// application code), so there was nothing existing to hook into. This
/// was confirmed with, and explicitly authorized by, the human operator
/// before implementation (see HANDOVER.md).
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
    ) -> Result<(), UpdateSubscriptionUsageError> {
        let mut subscription = self
            .subscription_repository
            .find_by_tenant_id(command.tenant_id)
            .await?
            .ok_or(UpdateSubscriptionUsageError::SubscriptionNotFound)?;

        // Domain validation (monitors/interval/retention > 0) happens
        // here, BEFORE anything is persisted.
        subscription.update_usage(
            command.monitors,
            command.interval_seconds,
            command.retention_days,
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
            command.monitors,
            command.interval_seconds,
            command.retention_days,
            now,
        );

        self.transaction
            .change_usage(
                &subscription,
                previous_open_period.map(|p| p.id),
                &new_period,
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Subscription;
    use domain::enums::PlanTier;
    use domain::shared::ids::SubscriptionId;
    use std::sync::Arc;

    // A minimal, self-contained fake IdGenerator/repos are intentionally
    // NOT built here to keep this unit test light; full behavioral
    // coverage (including the transaction wiring) lives in the
    // in-memory-backed integration test in `crates/infra/tests` and the
    // domain-level `update_usage` tests in
    // `crates/domain/src/entities/subscription.rs`. This test only
    // exercises the pure validation short-circuit path.

    struct RejectingTransaction;

    #[async_trait::async_trait]
    impl SubscriptionUsageTransaction for RejectingTransaction {
        async fn change_usage(
            &self,
            _subscription: &Subscription,
            _previous_open_period_id: Option<domain::shared::ids::SubscriptionUsagePeriodId>,
            _new_period: &SubscriptionUsagePeriod,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            panic!("transaction should never be called when domain validation fails");
        }
    }

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
    }

    #[tokio::test]
    async fn execute_rejects_zero_monitors_before_touching_the_transaction() {
        let subscription =
            Subscription::new(SubscriptionId::new(), TenantId::new(), PlanTier::Tier1);
        let tenant_id = subscription.tenant_id;

        let use_case = UpdateSubscriptionUsageUseCase::new(
            Arc::new(FixedSubscriptionRepository { subscription }),
            Arc::new(EmptyUsagePeriodRepository),
            Arc::new(NoopIdGenerator),
            RejectingTransaction,
        );

        let result = use_case
            .execute(UpdateSubscriptionUsageCommand {
                tenant_id,
                monitors: 0,
                interval_seconds: 60,
                retention_days: 30,
                pricing_version: 1,
            })
            .await;

        assert!(matches!(
            result,
            Err(UpdateSubscriptionUsageError::InvalidUsage(
                domain::DomainError::SubscriptionUsageMonitorsZero
            ))
        ));
    }
}
