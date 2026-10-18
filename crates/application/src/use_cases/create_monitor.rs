use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateMonitorRequest,
    errors::PlanLimitError,
    ports::{
        AuditLogEntryRepository, EntitlementResolver, IdGenerator, MonitorIntervalPolicyRepository,
        MonitorRepository, PricingConfigRepository, QuotaCheckOutcome, TenantRepository,
        WalletRepository,
    },
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, Entitlements, ExpectedStatus,
    HeartbeatToken, Host, Monitor, MonitorBuilder, MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    entities::{MAX_REGIONS_PER_MONITOR, MAX_UPTIME_MONITORS, MIN_UPTIME_INTERVAL_SECONDS},
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Hard cap on the number of assertions a single monitor may be created with.
pub const MAX_ASSERTIONS_PER_MONITOR: usize = 20;

#[derive(Debug)]
pub enum CreateMonitorError {
    Forbidden,
    /// TASK 2 — structured hard-quota rejection. Distinct from
    /// `Other`: carries the machine-readable quota id, current usage,
    /// plan maximum, and a suggested action.
    PlanLimitExceeded(PlanLimitError),
    TooManyAssertions { limit: usize },
    TenantSuspended,
    /// The resolver was not wired. This is a *deployment* bug (a
    /// production `main.rs` that forgot to call
    /// `.with_entitlement_resolver(...)`), not a customer error.
    EntitlementResolverMissing,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create monitors"
            ),
            Self::PlanLimitExceeded(err) => write!(f, "{err}"),
            Self::TooManyAssertions { limit } => write!(
                f,
                "too many assertions: a monitor allows at most {limit} assertions"
            ),
            Self::TenantSuspended => write!(f, "tenant is suspended"),
            Self::EntitlementResolverMissing => write!(
                f,
                "entitlement resolver is not configured; cannot enforce plan limits"
            ),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlanLimitExceeded(err) => Some(err),
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

pub struct CreateMonitorUseCase<R, G> {
    repository: R,
    id_generator: G,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
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
            entitlement_resolver: None,
            tenant_repository: None,
        }
    }

    /// TASK 2 — installs the authoritative `EntitlementResolver`.
    /// Production MUST call this; tests that don't get a resolver will
    /// fail closed with `EntitlementResolverMissing`.
    pub fn with_entitlement_resolver(mut self, resolver: Arc<dyn EntitlementResolver>) -> Self {
        self.entitlement_resolver = Some(resolver);
        self
    }

    /// Optional tenant-status check. Kept from the previous version;
    /// disabled unless explicitly wired. A suspended tenant is rejected
    /// before any quota work happens.
    pub fn with_tenant_repository(mut self, repository: Arc<dyn TenantRepository>) -> Self {
        self.tenant_repository = Some(repository);
        self
    }

    // ── Deprecated no-op builders ─────────────────────────────────
    //
    // These are retained so that handlers/tests we do not control yet
    // (bulk import, preview) still compile against this use case. They
    // have no effect: PAYG is no longer consulted at creation time.
    // TASK 8 will delete them along with the rest of the PAYG runtime.

    pub fn with_wallet_repository(self, _repository: Arc<dyn WalletRepository>) -> Self {
        self
    }

    pub fn with_interval_policy_repository(
        self,
        _repository: Arc<dyn MonitorIntervalPolicyRepository>,
    ) -> Self {
        self
    }

    pub fn with_pricing_config_repository(
        self,
        _repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
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

    /// Bulk-import entry point. Preserved from the previous version;
    /// uses the same entitlement-resolved quotas.
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

        // The single authoritative entitlement snapshot for this call.
        let entitlements = self.resolve_entitlements(tenant_id).await?;

        // ── Plan-level interval floor ────────────────────────────
        if request.interval_seconds < entitlements.min_uptime_interval_seconds {
            return Err(CreateMonitorError::PlanLimitExceeded(PlanLimitError::new(
                MIN_UPTIME_INTERVAL_SECONDS,
                u64::from(request.interval_seconds),
                u64::from(entitlements.min_uptime_interval_seconds),
                "upgrade_plan",
            )));
        }

        // ── Plan-level regions cap (stricter than the domain's hard 4) ──
        if (request.regions.len() as u64) > u64::from(entitlements.max_regions_per_monitor) {
            return Err(CreateMonitorError::PlanLimitExceeded(PlanLimitError::new(
                MAX_REGIONS_PER_MONITOR,
                request.regions.len() as u64,
                u64::from(entitlements.max_regions_per_monitor),
                "upgrade_plan",
            )));
        }

        let monitor = self
            .build_monitor(request, &authenticated_user, monitor_id)
            .await
            .map_err(CreateMonitorError::Other)?;

        // ── Atomic count-then-insert under the plan's quota ───────
        match self
            .repository
            .save_if_within_quota(&monitor, tenant_id, entitlements.max_uptime_monitors)
            .await
            .map_err(CreateMonitorError::Other)?
        {
            QuotaCheckOutcome::Saved => {}
            QuotaCheckOutcome::QuotaExceeded => {
                return Err(CreateMonitorError::PlanLimitExceeded(PlanLimitError::new(
                    MAX_UPTIME_MONITORS,
                    u64::from(entitlements.max_uptime_monitors),
                    u64::from(entitlements.max_uptime_monitors),
                    "upgrade_plan",
                )));
            }
        }

        self.record_audit_log(&monitor, user_id, tenant_id)
            .await
            .map_err(CreateMonitorError::Other)?;

        Ok(monitor)
    }

    async fn resolve_entitlements(
        &self,
        tenant_id: TenantId,
    ) -> Result<Entitlements, CreateMonitorError> {
        let resolver = self
            .entitlement_resolver
            .as_ref()
            .ok_or(CreateMonitorError::EntitlementResolverMissing)?;
        resolver
            .resolve(tenant_id)
            .await
            .map_err(CreateMonitorError::Other)
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
