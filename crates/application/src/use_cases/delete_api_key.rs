use std::{error::Error, sync::Arc};

use chrono::Utc;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::AuthenticatedUser,
};
use serde_json::json;

use crate::{
    commands::DeleteApiKey,
    ports::{ApiKeyRepository, AuditLogEntryRepository},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Deletes an API key through the application layer.
///
/// Tenant-wide access is enforced via `AuthorizationPolicy::same_tenant`
/// after fetching the entity — any member of the tenant may revoke any
/// key belonging to that tenant, not just its creator — then the key is
/// deleted and an audit entry is written. Mirrors
/// `DeleteNotificationChannelUseCase` exactly, minus that use case's
/// legacy single-owner check.
pub struct DeleteApiKeyUseCase {
    api_key_repository: Arc<dyn ApiKeyRepository>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl DeleteApiKeyUseCase {
    pub fn new(
        api_key_repository: Arc<dyn ApiKeyRepository>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            api_key_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: DeleteApiKey,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<bool> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let api_key = match self.api_key_repository.find_by_id(command.id).await? {
            Some(api_key) => api_key,
            None => return Ok(false),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            api_key.tenant_id(),
        ) {
            // Same response as "not found" — do not leak that a key with
            // this id exists in a different tenant.
            return Ok(false);
        }

        let deleted = self.api_key_repository.delete(command.id).await?;

        if deleted {
            let entry = AuditLogEntry {
                id: AuditLogEntryId::new(),
                actor_user_id: user_id,
                tenant_id: Some(tenant_id),
                action: AuditAction::ApiKeyDeleted,
                target_id: api_key.id.0.to_string(),
                metadata: json!({
                    "name": api_key.name,
                }),
                created_at: Utc::now(),
            };
            self.audit_log_entry_repository.create(&entry).await?;
        }

        Ok(deleted)
    }
}
