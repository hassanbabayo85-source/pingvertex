use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::shared::ids::ProxyCheckJobId;
use domain::shared::policies::AuthorizationPolicy;
use domain::shared::types::AuthenticatedUser;

use crate::ports::ProxyCheckJobRepository;

pub struct GetProxyCheckJobUseCase<R> {
    repository: R,
}

impl<R> GetProxyCheckJobUseCase<R>
where
    R: ProxyCheckJobRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id: ProxyCheckJobId,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<(ProxyCheckJob, Vec<ProxyEntry>)>, Box<dyn std::error::Error + Send + Sync>>
    {
        let job = self.repository.find_job_by_id(id).await?;

        match job {
            Some(job)
                if AuthorizationPolicy::same_tenant(
                    &authenticated_user.tenant_context(),
                    job.tenant_id(),
                ) =>
            {
                let entries = self.repository.find_entries_by_job_id(id).await?;
                Ok(Some((job, entries)))
            }
            _ => Ok(None),
        }
    }
}
