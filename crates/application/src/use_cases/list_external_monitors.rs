use std::error::Error;

use crate::pagination::{Page, PageRequest};
use crate::ports::MonitorRepository;
use domain::Monitor;
use domain::shared::ids::TenantId;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists monitors for external API key consumers.
///
/// Security boundary: an authenticated tenant MUST NOT be able
/// to force the API to load an unbounded amount of DB data in one
/// request. This delegates to `find_all_for_tenant_paginated` to limit
/// memory consumption to `MAX_PAGE_SIZE`.
pub struct ListExternalMonitorsUseCase<R> {
    repository: R,
}

impl<R> ListExternalMonitorsUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Cursor-paginated entry point for external HTTP list endpoints.
    pub async fn execute(
        &self,
        page: PageRequest,
        tenant_id: TenantId,
    ) -> UseCaseResult<Page<Monitor>> {
        self.repository
            .find_all_for_tenant_paginated(tenant_id, page)
            .await
    }
}
