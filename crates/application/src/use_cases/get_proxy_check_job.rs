use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::shared::ids::{ProxyCheckJobId, UserId};

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
        user_id: UserId,
    ) -> Result<Option<(ProxyCheckJob, Vec<ProxyEntry>)>, Box<dyn std::error::Error + Send + Sync>>
    {
        let job = self.repository.find_job_by_id(id).await?;

        match job {
            Some(job) if *job.user_id() == user_id => {
                let entries = self.repository.find_entries_by_job_id(id).await?;
                Ok(Some((job, entries)))
            }
            _ => Ok(None),
        }
    }
}
