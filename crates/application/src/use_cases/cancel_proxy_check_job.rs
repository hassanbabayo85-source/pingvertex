use crate::ports::ProxyCheckJobRepository;
use domain::shared::ids::ProxyCheckJobId;
use domain::shared::types::AuthenticatedUser;

pub struct CancelProxyCheckJobUseCase<R> {
    repository: R,
}
impl<R: ProxyCheckJobRepository> CancelProxyCheckJobUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
    pub async fn execute(
        &self,
        id: ProxyCheckJobId,
        user: AuthenticatedUser,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.repository
            .cancel_job(id, user.tenant_context().tenant_id())
            .await
    }
}
