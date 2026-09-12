use std::{error::Error, sync::Arc};

use domain::shared::types::AuthenticatedUser;

use crate::{dto::DashboardOverview, ports::DashboardRepository};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct GetDashboardOverviewUseCase {
    dashboard_repository: Arc<dyn DashboardRepository>,
}

impl GetDashboardOverviewUseCase {
    pub fn new(dashboard_repository: Arc<dyn DashboardRepository>) -> Self {
        Self {
            dashboard_repository,
        }
    }

    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<DashboardOverview> {
        self.dashboard_repository
            .get_overview(authenticated_user.tenant_context().tenant_id())
            .await
    }
}
