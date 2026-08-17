use std::{error::Error, fmt};

use crate::{
    commands::UploadStatusPageLogo,
    ports::{LogoStorage, StatusPageRepository},
};
use domain::{entities::StatusPage, errors::DomainError, value_objects::LogoImage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Errors from uploading a status page logo.
///
/// Distinguishes client-fixable input problems (`InvalidLogo` — wrong
/// file, too large, corrupt) from everything else (storage/database
/// failures), the same way `CreateStatusPageError` separates
/// `QuotaExceeded`/`SubdomainTaken` from `Other` — so the API layer can
/// return a `400 Bad Request` with a helpful message for the former
/// instead of an opaque `500` that hides what the caller needs to fix.
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
}

impl<R, S> UploadStatusPageLogoUseCase<R, S>
where
    R: StatusPageRepository,
    S: LogoStorage,
{
    pub fn new(repository: R, logo_storage: S) -> Self {
        Self {
            repository,
            logo_storage,
        }
    }

    pub async fn execute(
        &self,
        command: UploadStatusPageLogo,
    ) -> Result<Option<StatusPage>, UploadStatusPageLogoError> {
        let mut status_page = match self
            .repository
            .find_by_id_for_user(command.id, command.user_id)
            .await
            .map_err(UploadStatusPageLogoError::Other)?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        // Validate BEFORE touching storage — no point uploading bytes to
        // S3 for an image we're about to reject anyway. This is also the
        // one call in this method whose failure is the client's fault,
        // not ours, so it gets its own error variant.
        let logo = LogoImage::new(command.data).map_err(UploadStatusPageLogoError::InvalidLogo)?;

        self.finish_upload(&mut status_page, &logo)
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
