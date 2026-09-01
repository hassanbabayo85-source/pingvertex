use std::{error::Error, sync::Arc};

use chrono::Utc;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};
use serde_json::json;

use crate::{
    commands::DeleteApiKey,
    ports::{ApiKeyRepository, AuditLogEntryRepository},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Deletes an API key through the application layer.
///
/// Requires `Permission::ManageApiKeys`.
///
/// Tenant isolation is enforced after fetching the entity so that a key
/// belonging to another tenant is never revealed through this endpoint.
/// Permission is checked before the lookup to prevent low-privilege users
/// from using known key ids to revoke credentials.
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
        // Security: revoking API keys is a privileged tenant-management
        // operation. Check permission BEFORE loading the key so a
        // low-privilege user cannot use the endpoint even when they know
        // a valid key id.
        authenticated_user.require(Permission::ManageApiKeys)?;

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
