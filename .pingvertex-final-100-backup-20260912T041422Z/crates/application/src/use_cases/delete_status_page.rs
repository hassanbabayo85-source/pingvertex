use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::ports::{AuditLogEntryRepository, StatusPageRepository};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, StatusPageId,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct DeleteStatusPageUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> DeleteStatusPageUseCase<R>
where
    R: StatusPageRepository,
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

    /// Returns `true` if a status page belonging to the authenticated
    /// user's tenant was deleted, `false` if no such status page existed
    /// or if it belongs to a different tenant.
    pub async fn execute(
        &self,
        id: StatusPageId,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<bool> {
        authenticated_user.require(Permission::DeleteMonitor)?;

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let status_page = match self.repository.find_by_id(id).await? {
            Some(status_page) => status_page,
            None => return Ok(false),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            status_page.tenant_id(),
        ) {
            return Ok(false);
        }

        let deleted = self.repository.delete_for_tenant(id, tenant_id).await?;

        if deleted {
            let entry = AuditLogEntry {
                id: AuditLogEntryId::new(),
                actor_user_id: user_id,
                tenant_id: Some(tenant_id),
                action: AuditAction::StatusPageDeleted,
                target_id: status_page.id.0.to_string(),
                metadata: json!({
                    "subdomain": status_page.subdomain.as_str(),
                }),
                created_at: Utc::now(),
            };
            self.audit_log_entry_repository.create(&entry).await?;
        }

        Ok(deleted)
    }
}
