use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::ports::{AuditLogEntryRepository, MonitorRepository};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct SetMonitorEnabledCommand {
    pub id: domain::shared::ids::MonitorId,
    pub enabled: bool,
}

pub struct SetMonitorEnabledUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> SetMonitorEnabledUseCase<R>
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
        }
    }

    pub async fn execute(
        &self,
        command: SetMonitorEnabledCommand,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<domain::Monitor>> {
        authenticated_user.require(Permission::PauseMonitor)?;

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

        if command.enabled {
            monitor.enable();
        } else {
            monitor.disable();
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
                "enabled": command.enabled,
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(monitor))
    }
}
