use crate::ports::ProxyCheckJobRepository;
use domain::entities::ProxyCheckJob;
use domain::shared::ids::ProxyCheckJobId;
use domain::shared::types::AuthenticatedUser;

pub struct GetProxyCheckJobUseCase<R> {
    repository: R,
}
impl<R: ProxyCheckJobRepository> GetProxyCheckJobUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub async fn execute(
        &self,
        id: ProxyCheckJobId,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<ProxyCheckJob>, Box<dyn std::error::Error + Send + Sync>> {
        self.repository
            .find_job_by_id(id, authenticated_user.tenant_context().tenant_id())
            .await
    }
}
