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
}
