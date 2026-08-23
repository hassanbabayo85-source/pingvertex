use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateMonitor,
    ports::{AuditLogEntryRepository, MonitorRepository, SubscriptionRepository},
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, Host, Monitor,
    MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::AuthenticatedUser,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateMonitorUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    subscription_repository: Option<Arc<dyn SubscriptionRepository>>,
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
            subscription_repository: None,
        }
    }

    /// Enables plan-minimum interval validation during updates.
    /// When not called, falls back to global `CheckInterval::MIN`.
    pub fn with_entitlements(
        mut self,
        subscription_repository: Arc<dyn SubscriptionRepository>,
    ) -> Self {
        self.subscription_repository = Some(subscription_repository);
        self
    }

    pub async fn execute(
        &self,
        command: UpdateMonitor,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<Monitor>> {
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

        // Resolve min interval (plan or fallback)
        let min_interval = match &self.subscription_repository {
            Some(repo) => {
                let plan = ResolveEntitlementsUseCase::new(repo.clone())
                    .execute(ResolveEntitlements { tenant_id })
                    .await?;
                plan.min_check_interval_seconds
            }
            None => CheckInterval::MIN,
        };

        if command.interval_seconds < min_interval {
            return Err(format!(
                "interval must be at least {min_interval} seconds on your current plan"
            )
            .into());
        }

        // Validate target based on kind, same as create
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
