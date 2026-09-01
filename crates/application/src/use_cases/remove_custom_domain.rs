use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::RemoveCustomDomain,
    ports::{AuditLogEntryRepository, StatusPageRepository},
};
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct RemoveCustomDomainUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> RemoveCustomDomainUseCase<R>
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

    /// Removes whatever custom domain is currently attached (pending or
    /// verified) from the status page. Idempotent — calling this when no
    /// custom domain is set simply leaves the page unchanged and still
    /// returns `Ok(Some(status_page))`.
    pub async fn execute(
        &self,
        command: RemoveCustomDomain,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<StatusPage>> {
        authenticated_user.require(Permission::UpdateTenantSettings)?;

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut status_page = match self.repository.find_by_id(command.id).await? {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            status_page.tenant_id(),
        ) {
            return Ok(None);
        }

        let had_domain = status_page.custom_domain.clone();
        status_page.remove_custom_domain();
        self.repository.update(&status_page).await?;

        // Only write an audit entry if there was actually something to
        // remove — an idempotent no-op call shouldn't create audit noise.
        if let Some((domain, _)) = had_domain {
            let entry = AuditLogEntry {
                id: AuditLogEntryId::new(),
                actor_user_id: user_id,
                tenant_id: Some(tenant_id),
                action: AuditAction::CustomDomainRemoved,
                target_id: status_page.id.0.to_string(),
                metadata: json!({
                    "domain": domain.as_str(),
                }),
                created_at: Utc::now(),
            };
            self.audit_log_entry_repository.create(&entry).await?;
        }

        Ok(Some(status_page))
    }
}
