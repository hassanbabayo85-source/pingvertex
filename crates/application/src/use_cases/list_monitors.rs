use std::error::Error;

use crate::pagination::{Page, PageRequest};
use crate::{ports::MonitorRepository, queries::ListMonitors};
use domain::Monitor;
use domain::shared::types::AuthenticatedUser;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists monitors through the application layer.
///
/// The application layer is the security boundary that
/// guarantees "an authenticated tenant cannot force the API to load an
/// unbounded amount of DB data in one request". Both methods below
/// delegate to a paginated repository call; `execute` returns only the
/// first `MAX_PAGE_SIZE` monitors (never the whole set), and
/// `execute_paginated` exposes the full cursor interface for HTTP
/// handlers that want to walk multiple pages.
pub struct ListMonitorsUseCase<R> {
    repository: R,
}

impl<R> ListMonitorsUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Backward-compatible convenience entry point: returns the first
    /// `MAX_PAGE_SIZE` monitors for the caller's tenant, sorted
    /// `created_at DESC, id DESC`.
    ///
    /// Callers that need to page through everything MUST use
    /// `execute_paginated` — this method intentionally discards the
    /// `next_cursor` rather than exposing it, because it exists only to
    /// keep the old `Vec<Monitor>` return shape for internal callers and
    /// tests.
    pub async fn execute(
        &self,
        _query: ListMonitors,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Vec<Monitor>> {
        let page = self
            .execute_paginated(PageRequest::first(), authenticated_user)
            .await?;
        Ok(page.items)
    }

    /// Cursor-paginated entry point for HTTP list endpoints.
    pub async fn execute_paginated(
        &self,
        page: PageRequest,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Page<Monitor>> {
        self.repository
            .find_all_for_tenant_paginated(authenticated_user.tenant_context().tenant_id(), page)
            .await
    }
}
