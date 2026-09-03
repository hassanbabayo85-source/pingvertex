use std::error::Error;
use std::sync::Arc;

use application::{
    ports::{
        CheckRepository, DiagnosticReportRepository,
        check_repository::{CheckFilters, RepoResult as CheckRepoResult},
    },
    use_cases::ListChecksUseCase,
};
use async_trait::async_trait;
use domain::shared::ids::{CheckId, MonitorId, TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{Check, DiagnosticReport};

#[derive(Clone, Default)]
struct NoopCheckRepository;

#[async_trait]
impl CheckRepository for NoopCheckRepository {
    async fn save(&self, _: &Check) -> CheckRepoResult<()> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _: CheckId,
        _: chrono::DateTime<chrono::Utc>,
    ) -> CheckRepoResult<Option<Check>> {
        Ok(None)
    }

    async fn list_by_monitor(&self, _: MonitorId) -> CheckRepoResult<Vec<Check>> {
        Ok(vec![])
    }

    async fn list_checks(&self, _: u32, _: u32, _: TenantId) -> CheckRepoResult<Vec<Check>> {
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

    let tenant_id = TenantId::new();
    let user_id = UserId::from_uuid(uuid::Uuid::new_v4());
    let authenticated_user = AuthenticatedUser::new(
        user_id,
        Role::Owner,
        TenantContext::with_actor(tenant_id, user_id),
    );
    let result = use_case
        .execute(10, 0, authenticated_user, CheckFilters::default())
        .await
        .unwrap();

    assert!(result.is_empty());
}
