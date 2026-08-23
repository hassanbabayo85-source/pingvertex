use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateStatusPage,
    ports::{AuditLogEntryRepository, MonitorRepository, StatusPageRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, HexColor, MonitorId,
    entities::StatusPage,
    shared::ids::{TenantId, UserId},
    shared::policies::TenantOwned,
    shared::types::AuthenticatedUser,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum UpdateStatusPageError {
    MonitorNotFound { monitor_id: MonitorId },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for UpdateStatusPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MonitorNotFound { monitor_id } => {
                write!(f, "monitor {monitor_id} not found or access denied")
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UpdateStatusPageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MonitorNotFound { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

impl From<Box<dyn Error + Send + Sync>> for UpdateStatusPageError {
    fn from(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::Other(err)
    }
}

impl From<domain::errors::DomainError> for UpdateStatusPageError {
    fn from(err: domain::errors::DomainError) -> Self {
        Self::Other(Box::new(err))
    }
}

pub struct UpdateStatusPageUseCase<R, M> {
    repository: R,
    monitor_repository: M,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, M> UpdateStatusPageUseCase<R, M>
where
    R: StatusPageRepository,
    M: MonitorRepository,
{
    pub fn new(
        repository: R,
        monitor_repository: M,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            monitor_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: UpdateStatusPage,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<StatusPage>, UpdateStatusPageError> {
        self.execute_for_tenant(
            command,
            authenticated_user.tenant_context().tenant_id(),
            authenticated_user.user_id(),
        )
        .await
    }

    /// Tenant-scoped variant of `execute` that takes `tenant_id` /
    /// `acting_user_id` directly instead of an `AuthenticatedUser`.
    ///
    /// Added for the API-key-authenticated public surface
    /// (`PUT /api/v1/external/status-pages/{id}`, B31) — see the doc
    /// comment on `ListChecksUseCase::execute_for_tenant` for why a
    /// separate method exists instead of synthesizing an
    /// `AuthenticatedUser` from an `ApiKeyPrincipal`. `execute` above is
    /// now a thin wrapper around this.
    pub async fn execute_for_tenant(
        &self,
        command: UpdateStatusPage,
        tenant_id: TenantId,
        acting_user_id: UserId,
    ) -> Result<Option<StatusPage>, UpdateStatusPageError> {
        let mut status_page = match self.repository.find_by_id(command.id).await? {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        if status_page.tenant_id() != tenant_id {
            return Ok(None);
        }

        self.assert_monitors_owned_for_tenant(tenant_id, &command.monitor_ids)
            .await?;

        status_page.rename(command.name)?;
        status_page.set_monitors(command.monitor_ids)?;
        status_page.set_logo_url(command.logo_url)?;

        let primary_color = command.primary_color.map(HexColor::new).transpose()?;
        status_page.set_primary_color(primary_color);

        self.repository.update(&status_page).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: acting_user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::StatusPageUpdated,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "monitor_count": status_page.monitor_ids.len(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(status_page))
    }

    async fn assert_monitors_owned_for_tenant(
        &self,
        tenant_id: TenantId,
        monitor_ids: &[MonitorId],
    ) -> Result<(), UpdateStatusPageError> {
        for &monitor_id in monitor_ids {
            let monitor = self
                .monitor_repository
                .find_by_id(monitor_id)
                .await
                .map_err(UpdateStatusPageError::Other)?
                .ok_or(UpdateStatusPageError::MonitorNotFound { monitor_id })?;

            if monitor.tenant_id() != tenant_id {
                return Err(UpdateStatusPageError::MonitorNotFound { monitor_id });
            }
        }
        Ok(())
    }
}
