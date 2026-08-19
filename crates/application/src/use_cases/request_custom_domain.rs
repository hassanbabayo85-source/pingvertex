use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::RequestCustomDomain,
    ports::{AuditLogEntryRepository, StatusPageRepository},
};
use domain::shared::policies::OwnershipPolicy;
use domain::shared::types::AuthenticatedUser;
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, CustomDomain, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct RequestCustomDomainUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> RequestCustomDomainUseCase<R>
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

    pub async fn execute(
        &self,
        command: RequestCustomDomain,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<StatusPage>> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut status_page = match self
            .repository
            .find_by_id_for_user(command.id, user_id)
            .await?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        if !OwnershipPolicy::can_access(&authenticated_user, &status_page) {
            return Ok(None);
        }

        let domain = CustomDomain::new(command.domain)?;
        status_page.request_custom_domain(domain);

        self.repository.update(&status_page).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::CustomDomainRequested,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "subdomain": status_page.subdomain.as_str(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(status_page))
    }
}
