use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateMonitorRequest,
    ports::{
        AuditLogEntryRepository, IdGenerator, MonitorRepository, PricingConfigRepository,
        QuotaCheckOutcome, SubscriptionRepository,
    },
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, HeartbeatToken,
    Host, Monitor, MonitorBuilder, MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Backwards-compatible fallback cap used only when this use case is constructed
/// without `.with_entitlements(...)`. The production API always enables the
/// atomic, tenant-scoped entitlement path. Keep this value aligned with the
/// product Free-tier monitor limit so direct/unit-test construction cannot
/// accidentally reintroduce the old 10-monitor contract.
pub const MAX_MONITORS_PER_USER: usize = 20;

/// Backwards-compatible fallback minimum interval for direct/unit-test
/// construction without an entitlement repository. This mirrors the current
/// Free-tier contract; production requests resolve the value from the plan.
const DEFAULT_FREE_MIN_INTERVAL_SECONDS: u32 = 300;

/// Hard cap on the number of assertions a single monitor may be created with.
pub const MAX_ASSERTIONS_PER_MONITOR: usize = 20;

#[derive(Debug)]
pub enum CreateMonitorError {
    Forbidden,
    QuotaExceeded { limit: usize },
    TooManyAssertions { limit: usize },
    IntervalBelowPlanMinimum { min: u32 },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create monitors"
            ),
            Self::QuotaExceeded { limit } => {
                write!(
                    f,
                    "monitor limit reached: your plan allows at most {limit} monitors"
                )
            }
            Self::TooManyAssertions { limit } => {
                write!(
                    f,
                    "too many assertions: a monitor allows at most {limit} assertions"
                )
            }
            Self::IntervalBelowPlanMinimum { min } => {
                write!(
                    f,
                    "interval must be at least {min} seconds on your current plan"
                )
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Forbidden => None,
            Self::QuotaExceeded { .. } => None,
            Self::TooManyAssertions { .. } => None,
            Self::IntervalBelowPlanMinimum { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateMonitorUseCase<R, G> {
    repository: R,
    id_generator: G,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    subscription_repository: Option<Arc<dyn SubscriptionRepository>>,
    pricing_config_repository: Option<Arc<dyn PricingConfigRepository>>,
}

impl<R, G> CreateMonitorUseCase<R, G>
where
    R: MonitorRepository,
    G: IdGenerator,
{
    pub fn new(
        repository: R,
        id_generator: G,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            audit_log_entry_repository,
            subscription_repository: None,
            pricing_config_repository: None,
        }
    }

    /// Enables B13's atomic, tenant-scoped, plan-entitlement-based quota
    /// enforcement and plan-minimum interval validation.
    ///
    /// When this is NOT called, `execute` retains the legacy non-atomic
    /// fallback used by older direct/unit-test callers. Its limits are kept
    /// aligned with the current Free-tier contract.
    pub fn with_entitlements(
        mut self,
        subscription_repository: Arc<dyn SubscriptionRepository>,
    ) -> Self {
        self.subscription_repository = Some(subscription_repository);
        self
    }

    pub fn with_pricing_config_repository(
        mut self,
        repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_config_repository = Some(repository);
        self
    }

    pub async fn execute(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Monitor, CreateMonitorError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateMonitorError::Forbidden);
        }

        if request.assertions.len() > MAX_ASSERTIONS_PER_MONITOR {
            return Err(CreateMonitorError::TooManyAssertions {
                limit: MAX_ASSERTIONS_PER_MONITOR,
            });
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let (monitor_limit, min_interval) = match &self.subscription_repository {
            Some(repo) => {
                let plan = {
                    let mut resolver = ResolveEntitlementsUseCase::new(repo.clone());

                    if let Some(pricing) = &self.pricing_config_repository {
                        resolver = resolver.with_pricing_config_repository(pricing.clone());
                    }

                    resolver
                        .execute(ResolveEntitlements { tenant_id })
                        .await
                        .map_err(CreateMonitorError::Other)?
                };

                (plan.max_monitors, plan.min_check_interval_seconds)
            }
            None => (
                MAX_MONITORS_PER_USER as u32,
                DEFAULT_FREE_MIN_INTERVAL_SECONDS,
            ),
        };

        if request.interval_seconds < min_interval {
            return Err(CreateMonitorError::IntervalBelowPlanMinimum { min: min_interval });
        }

        let monitor = self
            .build_monitor(request, &authenticated_user)
            .await
            .map_err(CreateMonitorError::Other)?;

        match &self.subscription_repository {
            Some(_subscription_repository) => {
                match self
                    .repository
                    .save_if_within_quota(&monitor, tenant_id, monitor_limit)
                    .await
                    .map_err(CreateMonitorError::Other)?
                {
                    QuotaCheckOutcome::Saved => {}
                    QuotaCheckOutcome::QuotaExceeded => {
                        return Err(CreateMonitorError::QuotaExceeded {
                            limit: monitor_limit as usize,
                        });
                    }
                }
            }
            None => {
                let existing_for_user = self
                    .repository
                    .find_all_for_user(user_id)
                    .await
                    .map_err(CreateMonitorError::Other)?;

                if existing_for_user.len() >= monitor_limit as usize {
                    return Err(CreateMonitorError::QuotaExceeded {
                        limit: monitor_limit as usize,
                    });
                }

                self.repository
                    .save(&monitor)
                    .await
                    .map_err(CreateMonitorError::Other)?;
            }
        }

        self.record_audit_log(&monitor, user_id, tenant_id)
            .await
            .map_err(CreateMonitorError::Other)?;

        Ok(monitor)
    }

    async fn build_monitor(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: &AuthenticatedUser,
    ) -> UseCaseResult<Monitor> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let id = self.id_generator.next_monitor_id().await;

        let target = match request.kind {
            MonitorKind::Http | MonitorKind::Heartbeat => {
                Url::new(request.url)?.as_str().to_owned()
            }
            MonitorKind::Tcp | MonitorKind::Dns => Host::new(request.url)?.as_str().to_owned(),
            MonitorKind::Flow => request.url,
        };

        let mut builder = MonitorBuilder::new(id, user_id, tenant_id)
            .name(MonitorName::new(request.name)?)
            .target(target)
            .method(request.method)
            .expected_status(ExpectedStatus::new(request.expected_status)?)
            .interval(CheckInterval::new(request.interval_seconds)?)
            .timeout(Timeout::new(request.timeout_seconds)?)
            .kind(request.kind);

        if let Some(tcp_port) = request.tcp_port {
            builder = builder.tcp_port(tcp_port);
        }

        if let Some(dns_record_type) = request.dns_record_type {
            builder = builder.dns_record_type(dns_record_type);
        }

        if let Some(dns_expected_value) = request.dns_expected_value {
            builder = builder.dns_expected_value(dns_expected_value);
        }

        if request.kind == MonitorKind::Heartbeat {
            builder = builder.heartbeat_token(HeartbeatToken::generate());

            if let Some(interval) = request.heartbeat_interval_seconds {
                builder = builder.heartbeat_interval_seconds(interval);
            }

            if let Some(grace) = request.heartbeat_grace_period_seconds {
                builder = builder.heartbeat_grace_period_seconds(grace);
            }
        }

        let mut monitor = builder.try_build()?;

        monitor.is_public = request.is_public;
        monitor.ssl_check_enabled = request.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = request.ssl_expiry_warning_days;

        for input in &request.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

            monitor
                .add_assertion(assertion)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
        }

        Ok(monitor)
    }

    async fn record_audit_log(
        &self,
        monitor: &Monitor,
        user_id: UserId,
        tenant_id: TenantId,
    ) -> UseCaseResult<()> {
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorCreated,
            target_id: monitor.id.0.to_string(),
            metadata: json!({
                "name": monitor.name.as_str(),
                "is_public": monitor.is_public,
            }),
            created_at: Utc::now(),
        };

        self.audit_log_entry_repository.create(&entry).await?;
        Ok(())
    }
}
