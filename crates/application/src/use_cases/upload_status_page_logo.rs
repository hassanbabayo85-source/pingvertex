use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UploadStatusPageLogo,
    ports::{AuditLogEntryRepository, LogoStorage, StatusPageRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, entities::StatusPage, errors::DomainError,
    shared::policies::OwnershipPolicy, shared::types::AuthenticatedUser, value_objects::LogoImage,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum UploadStatusPageLogoError {
    InvalidLogo(DomainError),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for UploadStatusPageLogoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLogo(err) => write!(f, "{err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UploadStatusPageLogoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidLogo(err) => Some(err),
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct UploadStatusPageLogoUseCase<R, S> {
    repository: R,
    logo_storage: S,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, S> UploadStatusPageLogoUseCase<R, S>
where
    R: StatusPageRepository,
    S: LogoStorage,
{
    pub fn new(
        repository: R,
        logo_storage: S,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            logo_storage,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: UploadStatusPageLogo,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<StatusPage>, UploadStatusPageLogoError> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut status_page = match self
            .repository
            .find_by_id_for_user(command.id, user_id)
            .await
            .map_err(UploadStatusPageLogoError::Other)?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        if !OwnershipPolicy::can_access(&authenticated_user, &status_page) {
            return Ok(None);
        }

        let logo = LogoImage::new(command.data).map_err(UploadStatusPageLogoError::InvalidLogo)?;

        self.finish_upload(&mut status_page, &logo)
            .await
            .map_err(UploadStatusPageLogoError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::StatusPageUpdated,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "has_logo": true,
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(UploadStatusPageLogoError::Other)?;

        Ok(Some(status_page))
    }

    async fn finish_upload(
        &self,
        status_page: &mut StatusPage,
        logo: &LogoImage,
    ) -> UseCaseResult<()> {
        let logo_url = self.logo_storage.store_logo(logo).await?;
        status_page.set_logo_url(Some(logo_url))?;
        self.repository.update(status_page).await?;
        Ok(())
    }
}
