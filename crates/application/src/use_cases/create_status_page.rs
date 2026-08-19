use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateStatusPageRequest,
    ports::{AuditLogEntryRepository, IdGenerator, MonitorRepository, StatusPageRepository},
};
use domain::shared::policies::OwnershipPolicy;
use domain::shared::types::AuthenticatedUser;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, MonitorId, Subdomain,
    entities::{CreateStatusPage, StatusPage},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub const MAX_STATUS_PAGES_PER_USER: usize = 3;

#[derive(Debug)]
pub enum CreateStatusPageError {
    QuotaExceeded { limit: usize },
    SubdomainTaken { subdomain: String },
    MonitorNotFound { monitor_id: MonitorId },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateStatusPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => write!(
                f,
                "status page limit reached: at most {limit} status pages are allowed per account"
            ),
            Self::SubdomainTaken { subdomain } => {
                write!(f, "subdomain \"{subdomain}\" is already taken")
            }
            Self::MonitorNotFound { monitor_id } => {
                write!(f, "monitor {monitor_id} not found or access denied")
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateStatusPageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::QuotaExceeded { .. } => None,
            Self::SubdomainTaken { .. } => None,
            Self::MonitorNotFound { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateStatusPageUseCase<R, G, M> {
    repository: R,
    id_generator: G,
    monitor_repository: M,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, G, M> CreateStatusPageUseCase<R, G, M>
where
    R: StatusPageRepository,
    G: IdGenerator,
    M: MonitorRepository,
{
    pub fn new(
        repository: R,
        id_generator: G,
        monitor_repository: M,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            monitor_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        request: CreateStatusPageRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<StatusPage, CreateStatusPageError> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let subdomain = Subdomain::new(request.subdomain.clone())
            .map_err(|err| CreateStatusPageError::Other(Box::new(err)))?;

        let existing_pages = self
            .repository
            .find_all_for_user(user_id)
            .await
            .map_err(CreateStatusPageError::Other)?;

        if existing_pages.len() >= MAX_STATUS_PAGES_PER_USER {
            return Err(CreateStatusPageError::QuotaExceeded {
                limit: MAX_STATUS_PAGES_PER_USER,
            });
        }

        if self
            .repository
            .find_by_subdomain(subdomain.as_str())
            .await
            .map_err(CreateStatusPageError::Other)?
            .is_some()
        {
            return Err(CreateStatusPageError::SubdomainTaken {
                subdomain: subdomain.as_str().to_owned(),
            });
        }

        self.assert_monitors_owned(&authenticated_user, &request.monitor_ids)
            .await?;

        let status_page = self
            .build_and_save(request, subdomain, user_id, tenant_id)
            .await
            .map_err(CreateStatusPageError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::StatusPageCreated,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "subdomain": status_page.subdomain.as_str(),
                "monitor_count": status_page.monitor_ids.len(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(CreateStatusPageError::Other)?;

        Ok(status_page)
    }

    async fn assert_monitors_owned(
        &self,
        authenticated_user: &AuthenticatedUser,
        monitor_ids: &[MonitorId],
    ) -> Result<(), CreateStatusPageError> {
        for &monitor_id in monitor_ids {
            let monitor = self
                .monitor_repository
                .find_by_id_for_user(monitor_id, authenticated_user.user_id())
                .await
                .map_err(CreateStatusPageError::Other)?
                .ok_or(CreateStatusPageError::MonitorNotFound { monitor_id })?;

            if !OwnershipPolicy::can_access(authenticated_user, &monitor) {
                return Err(CreateStatusPageError::MonitorNotFound { monitor_id });
            }
        }

        Ok(())
    }

    async fn build_and_save(
        &self,
        request: CreateStatusPageRequest,
        subdomain: Subdomain,
        user_id: domain::UserId,
        tenant_id: domain::TenantId,
    ) -> UseCaseResult<StatusPage> {
        let id = self.id_generator.next_status_page_id().await;

        let status_page = StatusPage::create(CreateStatusPage {
            id,
            user_id,
            tenant_id,
            name: request.name,
            subdomain,
            monitor_ids: request.monitor_ids,
        })?;

        self.repository.save(&status_page).await?;

        Ok(status_page)
    }
}
