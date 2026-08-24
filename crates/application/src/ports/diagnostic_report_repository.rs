use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use domain::DiagnosticReport;
use domain::shared::ids::CheckId;

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Abstraction for diagnostic report persistence. One report per failed
/// `Check` (see `DiagnosticReport::check_result_id`).
#[async_trait]
pub trait DiagnosticReportRepository: Send + Sync {
    async fn save(&self, report: &DiagnosticReport) -> RepoResult<()>;

    async fn find_by_check_id(&self, check_id: CheckId) -> RepoResult<Option<DiagnosticReport>>;

    /// Bulk equivalent of `find_by_check_id`.
    ///
    /// The default implementation is deliberately safe-but-naive (one
    /// query per id) so existing mocks continue to compile unchanged.
    /// Postgres overrides this with a single `ANY($1)` query to avoid
    /// N+1 when materializing a page of checks (see
    /// `ListChecksUseCase`).
    async fn find_by_check_ids(
        &self,
        check_ids: &[CheckId],
    ) -> RepoResult<HashMap<CheckId, DiagnosticReport>> {
        let mut reports = HashMap::new();
        for check_id in check_ids {
            if let Some(report) = self.find_by_check_id(*check_id).await? {
                reports.insert(*check_id, report);
            }
        }
        Ok(reports)
    }
}

#[async_trait]
impl<T> DiagnosticReportRepository for Arc<T>
where
    T: DiagnosticReportRepository + ?Sized + Send + Sync,
{
    async fn save(&self, report: &DiagnosticReport) -> RepoResult<()> {
        (**self).save(report).await
    }

    async fn find_by_check_id(&self, check_id: CheckId) -> RepoResult<Option<DiagnosticReport>> {
        (**self).find_by_check_id(check_id).await
    }

    async fn find_by_check_ids(
        &self,
        check_ids: &[CheckId],
    ) -> RepoResult<HashMap<CheckId, DiagnosticReport>> {
        (**self).find_by_check_ids(check_ids).await
    }
}
