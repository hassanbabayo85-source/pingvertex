use std::{error::Error, fmt};

use crate::{dto::CreateStatusPageRequest, ports::IdGenerator, ports::StatusPageRepository};
use domain::{
    Subdomain,
    entities::{CreateStatusPage, StatusPage},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Hard cap on the number of status pages a single user may create.
///
/// Each status page carries its own polling (public endpoint traffic),
/// cache entries, and — once a custom domain is attached — a recurring
/// background DNS verification check. Without a limit a single account
/// could create an unbounded number of these, each with its own ongoing
/// cost. 3 comfortably covers legitimate use (most teams need one, maybe a
/// couple for different products) while blocking abuse.
pub const MAX_STATUS_PAGES_PER_USER: usize = 3;

#[derive(Debug)]
pub enum CreateStatusPageError {
    QuotaExceeded { limit: usize },
    SubdomainTaken { subdomain: String },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateStatusPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => {
                write!(
                    f,
                    "status page limit reached: at most {limit} status pages are allowed per account"
                )
            }
            Self::SubdomainTaken { subdomain } => {
                write!(f, "subdomain \"{subdomain}\" is already taken")
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
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateStatusPageUseCase<R, G> {
    repository: R,
    id_generator: G,
}

impl<R, G> CreateStatusPageUseCase<R, G>
where
    R: StatusPageRepository,
    G: IdGenerator,
{
    pub fn new(repository: R, id_generator: G) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        request: CreateStatusPageRequest,
    ) -> Result<StatusPage, CreateStatusPageError> {
        // Validate the subdomain shape first — cheap, in-memory, no
        // database round-trip — before doing any repository work.
        let subdomain = Subdomain::new(request.subdomain.clone())
            .map_err(|err| CreateStatusPageError::Other(Box::new(err)))?;

        let existing_pages = self
            .repository
            .find_all_for_user(request.user_id)
            .await
            .map_err(CreateStatusPageError::Other)?;

        if existing_pages.len() >= MAX_STATUS_PAGES_PER_USER {
            return Err(CreateStatusPageError::QuotaExceeded {
                limit: MAX_STATUS_PAGES_PER_USER,
            });
        }

        // Friendly pre-check before hitting the DB unique-index collision.
        // Still relies on the DB constraint as the real source of truth
        // (this check has an inherent TOCTOU race under concurrent
        // requests for the same subdomain), but it turns the common case
        // into a clean, specific error instead of a generic DB error.
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

        self.build_and_save(request, subdomain)
            .await
            .map_err(CreateStatusPageError::Other)
    }

    async fn build_and_save(
        &self,
        request: CreateStatusPageRequest,
        subdomain: Subdomain,
    ) -> UseCaseResult<StatusPage> {
        let id = self.id_generator.next_status_page_id().await;

        let status_page = StatusPage::create(CreateStatusPage {
            id,
            user_id: request.user_id,
            name: request.name,
            subdomain,
            monitor_ids: request.monitor_ids,
        })?;

        self.repository.save(&status_page).await?;

        Ok(status_page)
    }
}
