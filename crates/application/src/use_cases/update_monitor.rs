use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateMonitor,
    errors::PlanLimitError,
    ports::{
        AuditLogEntryRepository, EntitlementResolver, MonitorIntervalPolicyRepository,
        MonitorRepository, PricingConfigRepository, WalletRepository,
    },
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, Host, Monitor,
    MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    entities::MIN_UPTIME_INTERVAL_SECONDS,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateMonitorUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
}

impl<R> UpdateMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(
        repository: R,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            audit_log_entry_repository,
            entitlement_resolver: None,
        }
    }

    pub fn with_entitlement_resolver(mut self, resolver: Arc<dyn EntitlementResolver>) -> Self {
        self.entitlement_resolver = Some(resolver);
        self
    }

    // Deprecated no-ops (see `CreateMonitorUseCase` for the rationale).
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
        command: UpdateMonitor,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<Monitor>> {
        authenticated_user.require(Permission::UpdateMonitor)?;

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut monitor = match self.repository.find_by_id(command.id).await? {
            Some(monitor) => monitor,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Ok(None);
        }

        let resolver = self.entitlement_resolver.as_ref().ok_or_else(|| {
            Box::<dyn Error + Send + Sync>::from(
                "entitlement resolver is not configured; cannot enforce plan limits",
            )
        })?;
        let entitlements = resolver.resolve(tenant_id).await?;

        if command.interval_seconds < entitlements.min_uptime_interval_seconds {
            let err = PlanLimitError::new(
                MIN_UPTIME_INTERVAL_SECONDS,
                u64::from(command.interval_seconds),
                u64::from(entitlements.min_uptime_interval_seconds),
                "upgrade_plan",
            );
            return Err(Box::new(err));
        }

        let target = match command.kind {
            MonitorKind::Http | MonitorKind::Heartbeat => {
                Url::new(command.url)?.as_str().to_owned()
            }
            MonitorKind::Tcp | MonitorKind::Dns => Host::new(command.url)?.as_str().to_owned(),
            MonitorKind::Flow => command.url,
        };

        monitor.update(
            MonitorName::new(command.name)?,
            target,
            command.method,
            ExpectedStatus::new(command.expected_status)?,
            CheckInterval::new(command.interval_seconds)?,
            Timeout::new(command.timeout_seconds)?,
        );

        monitor
            .update_kind_fields(
                command.kind,
                command.tcp_port,
                command.dns_record_type,
                command.dns_expected_value,
                command.heartbeat_interval_seconds,
                command.heartbeat_grace_period_seconds,
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        monitor.is_public = command.is_public;
        monitor.ssl_check_enabled = command.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = command.ssl_expiry_warning_days;

        monitor.assertions.clear();
        for input in &command.assertions {
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

        self.repository.save(&monitor).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorUpdated,
            target_id: monitor.id.0.to_string(),
            metadata: json!({
                "name": monitor.name.as_str(),
                "is_public": monitor.is_public,
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(monitor))
    }
}
