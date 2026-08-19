use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::DeleteMonitor,
    ports::{AuditLogEntryRepository, MonitorRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, shared::policies::OwnershipPolicy,
    shared::types::AuthenticatedUser,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct DeleteMonitorUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> DeleteMonitorUseCase<R>
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
        command: DeleteMonitor,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<bool> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let monitor = match self.repository.find_by_id(command.id).await? {
            Some(monitor) => monitor,
            None => return Ok(false),
        };

        if !OwnershipPolicy::can_access(&authenticated_user, &monitor) {
            return Ok(false);
        }

        let deleted = self.repository.delete(command.id).await?;

        if deleted {
            let entry = AuditLogEntry {
                id: AuditLogEntryId::new(),
                actor_user_id: user_id,
                tenant_id: Some(tenant_id),
                action: AuditAction::MonitorDeleted,
                target_id: monitor.id.0.to_string(),
                metadata: json!({
                    "name": monitor.name.as_str(),
                }),
                created_at: Utc::now(),
            };
            self.audit_log_entry_repository.create(&entry).await?;
        }

        Ok(deleted)
    }
}
