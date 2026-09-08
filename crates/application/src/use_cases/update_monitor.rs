use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateMonitor,
    ports::{AuditLogEntryRepository, MonitorIntervalPolicyRepository, MonitorRepository, WalletRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, Host, Monitor,
    MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateMonitorUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    wallet_repository: Option<Arc<dyn WalletRepository>>,
    interval_policy_repository: Option<Arc<dyn MonitorIntervalPolicyRepository>>,
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
            wallet_repository: None,
            interval_policy_repository: None,
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

        let funded = if let Some(wallets) = &self.wallet_repository {
            let wallet = wallets.get_or_create(tenant_id.0, "USD").await?;
            // PAYG activation remains in force even when the current wallet
            // balance is zero. Balance is an execution authorization check,
            // not a tier downgrade signal.
            wallet.payg_activated_at.is_some()
        } else {
            false
        };
        let min_interval = if funded {
            if let Some(policies) = &self.interval_policy_repository {
                policies.minimum_interval_seconds(command.kind).await?
            } else {
                match command.kind {
                    MonitorKind::Flow => 60,
                    _ => CheckInterval::MIN,
                }
            }
        } else {
            domain::FREE_MIN_INTERVAL_SECONDS
        };

        if command.interval_seconds < min_interval {
            return Err(format!(
                "interval must be at least {min_interval} seconds for the current billing eligibility state"
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

