use std::error::Error;

use crate::{commands::RequestCustomDomain, ports::StatusPageRepository};
use domain::{CustomDomain, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct RequestCustomDomainUseCase<R> {
    repository: R,
}

impl<R> RequestCustomDomainUseCase<R>
where
    R: StatusPageRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Begins the domain-ownership verification flow: validates the
    /// domain's syntax and stores it as `Pending` with a fresh TXT-record
    /// token. Does NOT verify ownership itself — that only happens when
    /// the (not-yet-built) background worker confirms the TXT record via
    /// a live DNS lookup and calls `mark_custom_domain_verified`.
    pub async fn execute(&self, command: RequestCustomDomain) -> UseCaseResult<Option<StatusPage>> {
        let mut status_page = match self
            .repository
            .find_by_id_for_user(command.id, command.user_id)
            .await?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        let domain = CustomDomain::new(command.domain)?;
        status_page.request_custom_domain(domain);

        self.repository.update(&status_page).await?;

        Ok(Some(status_page))
    }
}
