use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::DeleteApiFlow,
    ports::{ApiFlowRepository, AuditLogEntryRepository, MonitorRepository},
};
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, shared::types::AuthenticatedUser};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct DeleteApiFlowUseCase<R, M> {
    api_flow_repository: R,
    monitor_repository: M,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, M> DeleteApiFlowUseCase<R, M>
where
    R: ApiFlowRepository,
    M: MonitorRepository,
{
    pub fn new(
        api_flow_repository: R,
        monitor_repository: M,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            api_flow_repository,
            monitor_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: DeleteApiFlow,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<bool> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        if self
            .monitor_repository
            .find_by_id_for_user(command.monitor_id, user_id)
            .await?
            .is_none()
        {
            return Ok(false);
        }

        let flow = match self.api_flow_repository.find_by_id(command.id).await? {
            Some(flow) => flow,
            None => return Ok(false),
        };

        if flow.monitor_id != command.monitor_id {
            return Ok(false);
        }

        let deleted = self.api_flow_repository.delete(command.id).await?;

        if deleted {
            let entry = AuditLogEntry {
                id: AuditLogEntryId::new(),
                actor_user_id: user_id,
                tenant_id: Some(tenant_id),
                action: AuditAction::ApiFlowDeleted,
                target_id: flow.id.0.to_string(),
                metadata: json!({
                    "monitor_id": flow.monitor_id.0.to_string(),
                }),
                created_at: Utc::now(),
            };
            self.audit_log_entry_repository.create(&entry).await?;
        }

        Ok(deleted)
    }
}
