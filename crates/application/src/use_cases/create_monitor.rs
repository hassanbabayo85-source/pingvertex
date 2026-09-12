use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateMonitorRequest,
    ports::{
        AuditLogEntryRepository, IdGenerator, MonitorIntervalPolicyRepository, MonitorRepository,
        PricingConfigRepository, QuotaCheckOutcome, TenantRepository, WalletRepository,
    },
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, HeartbeatToken,
    Host, Monitor, MonitorBuilder, MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Hard cap on the number of assertions a single monitor may be created with.
pub const MAX_ASSERTIONS_PER_MONITOR: usize = 20;

#[derive(Debug)]
pub enum CreateMonitorError {
    Forbidden,
    QuotaExceeded { limit: usize },
    TooManyAssertions { limit: usize },
    IntervalBelowPlanMinimum { min: u32 },
    TenantSuspended,
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
                    "Free entitlement allows at most {limit} monitors; fund the PAYG wallet to add more."
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
                    "interval must be at least {min} seconds for the current billing eligibility state"
                )
            }
            Self::TenantSuspended => write!(f, "tenant is suspended"),
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
            Self::TenantSuspended => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateMonitorUseCase<R, G> {
    repository: R,
    id_generator: G,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    wallet_repository: Option<Arc<dyn WalletRepository>>,
    interval_policy_repository: Option<Arc<dyn MonitorIntervalPolicyRepository>>,
    pricing_config_repository: Option<Arc<dyn PricingConfigRepository>>,
    tenant_repository: Option<Arc<dyn TenantRepository>>,
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
            wallet_repository: None,
            interval_policy_repository: None,
            pricing_config_repository: None,
            tenant_repository: None,
        }
    }

    pub fn with_wallet_repository(mut self, repository: Arc<dyn WalletRepository>) -> Self {
        self.wallet_repository = Some(repository);
        self
    }

    pub fn with_interval_policy_repository(
        mut self,
        repository: Arc<dyn MonitorIntervalPolicyRepository>,
    ) -> Self {
        self.interval_policy_repository = Some(repository);
        self
    }

    pub fn with_pricing_config_repository(
        mut self,
        repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_config_repository = Some(repository);
        self
    }

    pub fn with_tenant_repository(mut self, repository: Arc<dyn TenantRepository>) -> Self {
        self.tenant_repository = Some(repository);
        self
    }

    pub async fn execute(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Monitor, CreateMonitorError> {
        let monitor_id = self.id_generator.next_monitor_id().await;
        self.execute_with_monitor_id(request, authenticated_user, monitor_id)
            .await
    }

    /// Bulk-import execution uses a durable, preallocated monitor id so a
    /// worker crash after INSERT but before row-state acknowledgement can be
    /// retried without creating a duplicate monitor.
    pub async fn execute_with_monitor_id(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: AuthenticatedUser,
        monitor_id: domain::shared::ids::MonitorId,
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

        if let Some(tenant_repository) = &self.tenant_repository {
            let tenant = tenant_repository
                .find_by_id(tenant_id)
                .await
                .map_err(CreateMonitorError::Other)?
                .ok_or(CreateMonitorError::TenantSuspended)?;
            if tenant.status != domain::TenantStatus::Active {
                return Err(CreateMonitorError::TenantSuspended);
            }
        }

        let funded = if let Some(wallets) = &self.wallet_repository {
            let wallet = wallets
                .get_or_create(tenant_id.0, "USD")
                .await
                .map_err(CreateMonitorError::Other)?;
            // PAYG activation is sticky. An exhausted PAYG wallet must not
            // silently regain the Free entitlement.
            wallet.payg_activated_at.is_some()
        } else {
            false
        };

        // PAYG is the runtime billing model. Before a wallet is activated,
        // the only runtime entitlement is the active pricing configuration's
        // free tier. Subscription plans are not consulted for monitor creation.
        let effective_plan = if let Some(pricing_repo) = &self.pricing_config_repository {
            if let Some(config) = pricing_repo
                .find_active_config()
                .await
                .map_err(CreateMonitorError::Other)?
            {
                let mut plan = domain::Plan::for_tier(domain::enums::PlanTier::Free);
                plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                plan.history_retention_days = config.free_tier.retention_days;
                plan
            } else {
                domain::Plan::for_tier(domain::enums::PlanTier::Free)
            }
        } else {
            domain::Plan::for_tier(domain::enums::PlanTier::Free)
        };

        let max_monitors_limit = effective_plan.max_monitors as usize;
        let default_free_min_interval = effective_plan.min_check_interval_seconds;

        let min_interval = if funded {
            if let Some(policies) = &self.interval_policy_repository {
                policies
                    .minimum_interval_seconds(request.kind)
                    .await
                    .map_err(CreateMonitorError::Other)?
            } else {
                match request.kind {
                    MonitorKind::Flow => 60,
                    _ => CheckInterval::MIN,
                }
            }
        } else {
            default_free_min_interval
        };

        if request.interval_seconds < min_interval {
            return Err(CreateMonitorError::IntervalBelowPlanMinimum { min: min_interval });
        }

        let monitor = self
            .build_monitor(request, &authenticated_user, monitor_id)
            .await
            .map_err(CreateMonitorError::Other)?;

        if funded {
            self.repository
                .save(&monitor)
                .await
                .map_err(CreateMonitorError::Other)?;
        } else {
            match self
                .repository
                .save_if_within_quota(&monitor, tenant_id, max_monitors_limit as u32)
                .await
                .map_err(CreateMonitorError::Other)?
            {
                QuotaCheckOutcome::Saved => {}
                QuotaCheckOutcome::QuotaExceeded => {
                    return Err(CreateMonitorError::QuotaExceeded {
                        limit: max_monitors_limit,
                    });
                }
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
        id: domain::shared::ids::MonitorId,
    ) -> UseCaseResult<Monitor> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

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

        if !request.regions.is_empty() {
            monitor.regions = request.regions;
        }
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
