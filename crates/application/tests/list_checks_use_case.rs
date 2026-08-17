use std::error::Error;
use std::sync::Arc;

use application::{
    ports::{
        CheckRepository, DiagnosticReportRepository,
        check_repository::RepoResult as CheckRepoResult,
    },
    use_cases::ListChecksUseCase,
};
use async_trait::async_trait;
use domain::shared::ids::{CheckId, MonitorId, UserId};
use domain::{Check, DiagnosticReport};

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

    async fn list_checks(&self, _: u32, _: u32, _: UserId) -> CheckRepoResult<Vec<Check>> {
        Ok(vec![])
    }

    async fn delete_by_monitor(&self, _: MonitorId) -> CheckRepoResult<u64> {
        Ok(0)
    }
}

#[derive(Clone, Default)]
struct NoopDiagnosticReportRepository;

#[async_trait]
impl DiagnosticReportRepository for NoopDiagnosticReportRepository {
    async fn save(&self, _: &DiagnosticReport) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_check_id(
        &self,
        _: CheckId,
    ) -> Result<Option<DiagnosticReport>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }
}

#[tokio::test]
async fn lists_checks() {
    let repository = Arc::new(NoopCheckRepository);
    let diagnostic_repository = Arc::new(NoopDiagnosticReportRepository);

    let use_case = ListChecksUseCase::new(repository, diagnostic_repository);

    let user_id = UserId::from_uuid(uuid::Uuid::new_v4());
    let result = use_case.execute(10, 0, user_id).await.unwrap();

    assert!(result.is_empty());
}
