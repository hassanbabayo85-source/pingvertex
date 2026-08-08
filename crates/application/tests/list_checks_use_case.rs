use std::sync::Arc;

use application::{
    ports::{CheckRepository, check_repository::RepoResult as CheckRepoResult},
    use_cases::ListChecksUseCase,
};
use async_trait::async_trait;
use domain::Check;
use domain::shared::ids::{CheckId, MonitorId};

#[derive(Clone, Default)]
struct NoopCheckRepository;

#[async_trait]
impl CheckRepository for NoopCheckRepository {
    async fn save(&self, _: &Check) -> CheckRepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _: CheckId) -> CheckRepoResult<Option<Check>> {
        Ok(None)
    }

    async fn list_by_monitor(&self, _: MonitorId) -> CheckRepoResult<Vec<Check>> {
        Ok(vec![])
    }

    async fn list_checks(&self, _: u32, _: u32) -> CheckRepoResult<Vec<Check>> {
        Ok(vec![])
    }

    async fn delete_by_monitor(&self, _: MonitorId) -> CheckRepoResult<u64> {
        Ok(0)
    }
}

#[tokio::test]
async fn lists_checks() {
    let repository = Arc::new(NoopCheckRepository);
    let use_case = ListChecksUseCase::new(repository);

    let result = use_case.execute(10, 0).await.unwrap();

    assert!(result.is_empty());
}
