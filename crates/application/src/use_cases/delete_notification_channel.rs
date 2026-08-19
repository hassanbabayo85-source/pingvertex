use std::{error::Error, sync::Arc};

use chrono::Utc;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, shared::policies::OwnershipPolicy,
    shared::types::AuthenticatedUser,
};
use serde_json::json;

use crate::{
    commands::DeleteNotificationChannel,
    ports::{AuditLogEntryRepository, NotificationChannelRepository},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Deletes a notification channel through the application layer.
///
/// Ownership is enforced via `OwnershipPolicy::can_access` after fetching
/// the entity, then the channel is deleted and an audit entry is written.
pub struct DeleteNotificationChannelUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> DeleteNotificationChannelUseCase<R>
where
    R: NotificationChannelRepository,
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
        command: DeleteNotificationChannel,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<bool> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let channel = match self
            .repository
            .find_by_id_for_user(command.id, user_id)
            .await?
        {
            Some(channel) => channel,
            None => return Ok(false),
        };

        if !OwnershipPolicy::can_access(&authenticated_user, &channel) {
            return Ok(false);
        }

        let deleted = self.repository.delete_for_user(command.id, user_id).await?;

        if deleted {
            let entry = AuditLogEntry {
                id: AuditLogEntryId::new(),
                actor_user_id: user_id,
                tenant_id: Some(tenant_id),
                action: AuditAction::NotificationChannelDeleted,
                target_id: channel.id.0.to_string(),
                metadata: json!({
                    "monitor_id": channel.monitor_id.0.to_string(),
                }),
                created_at: Utc::now(),
            };
            self.audit_log_entry_repository.create(&entry).await?;
        }

        Ok(deleted)
    }
}
