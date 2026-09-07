use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    MeterUsageInput, MeterUsageUseCase,
    commands::UploadStatusPageLogo,
    ports::{AuditLogEntryRepository, LogoStorage, StatusPageRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId,
    entities::StatusPage,
    errors::DomainError,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
    value_objects::LogoImage,
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum UploadStatusPageLogoError {
    Forbidden,
    InsufficientBalance,
    InvalidLogo(DomainError),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for UploadStatusPageLogoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to update status pages"
            ),
            Self::InsufficientBalance => write!(f, "insufficient PAYG balance"),
            Self::InvalidLogo(err) => write!(f, "{err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UploadStatusPageLogoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Forbidden | Self::InsufficientBalance => None,
            Self::InvalidLogo(err) => Some(err),
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct UploadStatusPageLogoUseCase<R, S> {
    repository: R,
    logo_storage: S,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    metering: Option<Arc<MeterUsageUseCase>>,
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
            metering: None,
        }
    }

    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub async fn execute(
        &self,
        command: UploadStatusPageLogo,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<StatusPage>, UploadStatusPageLogoError> {
        let request_id = format!("status-page-logo:{}:{}", command.id.0, uuid::Uuid::new_v4());
        self.execute_with_request_id(command, authenticated_user, request_id)
            .await
    }

    pub async fn execute_with_request_id(
        &self,
        command: UploadStatusPageLogo,
        authenticated_user: AuthenticatedUser,
        request_id: String,
    ) -> Result<Option<StatusPage>, UploadStatusPageLogoError> {
        if !authenticated_user.can(Permission::UpdateMonitor) {
            return Err(UploadStatusPageLogoError::Forbidden);
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut status_page = match self
            .repository
            .find_by_id(command.id)
            .await
            .map_err(UploadStatusPageLogoError::Other)?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            status_page.tenant_id(),
        ) {
            return Ok(None);
        }

        let logo = LogoImage::new(command.data).map_err(UploadStatusPageLogoError::InvalidLogo)?;
        let storage_units = ((logo.data().len() as u64).div_ceil(1024 * 1024)) as i64;
        let status_page_id = status_page.id;

        if let Some(metering) = &self.metering {
            // `MeterUsageUseCase::execute` requires an owned FnOnce callback.
            // Moving the already-loaded aggregate into that callback avoids an
            // async borrow escaping the closure. The aggregate is reloaded after
            // the metered operation so the caller receives the persisted state.
            let mut status_page_for_upload = Some(status_page);
            let mut logo_for_upload = Some(logo);
            metering
                .execute(
                    MeterUsageInput {
                        tenant_id: tenant_id.0,
                        operation: "storage".to_owned(),
                        units: storage_units.max(1),
                        request_id: format!("storage:{}", request_id),
                    },
                    move || async move {
                        let mut page = status_page_for_upload.take().ok_or_else(|| {
                            "status page upload operation invoked twice".to_owned()
                        })?;
                        let logo = logo_for_upload.take().ok_or_else(|| {
                            "status page logo upload operation invoked twice".to_owned()
                        })?;
                        self.finish_upload(&mut page, &logo).await
                    },
                )
                .await
                .map_err(|err| match err {
                    crate::use_cases::ChargeForUsageError::InsufficientBalance => {
                        UploadStatusPageLogoError::InsufficientBalance
                    }
                    other => UploadStatusPageLogoError::Other(Box::new(other)),
                })?;

            status_page = self
                .repository
                .find_by_id(status_page_id)
                .await
                .map_err(UploadStatusPageLogoError::Other)?
                .ok_or_else(|| {
                    UploadStatusPageLogoError::Other(
                        "status page disappeared after logo upload".into(),
                    )
                })?;
        } else {
            self.finish_upload(&mut status_page, &logo)
                .await
                .map_err(UploadStatusPageLogoError::Other)?;
        }

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::StatusPageUpdated,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "has_logo": true,
                "storage_billable_mb": storage_units.max(1),
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
