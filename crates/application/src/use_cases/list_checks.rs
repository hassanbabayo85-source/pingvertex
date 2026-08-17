use std::{error::Error, sync::Arc};

use crate::dto::CheckWithDiagnosticReport;
use crate::ports::{CheckRepository, DiagnosticReportRepository};
use domain::shared::ids::UserId;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists all checks belonging to the authenticated user, each paired with
/// its diagnostic report (if one exists for that failed check).
#[derive(Clone)]
pub struct ListChecksUseCase {
    check_repository: Arc<dyn CheckRepository>,
    diagnostic_report_repository: Arc<dyn DiagnosticReportRepository>,
}

impl ListChecksUseCase {
    pub fn new(
        check_repository: Arc<dyn CheckRepository>,
        diagnostic_report_repository: Arc<dyn DiagnosticReportRepository>,
    ) -> Self {
        Self {
            check_repository,
            diagnostic_report_repository,
        }
    }

    pub async fn execute(
        &self,
        limit: u32,
        offset: u32,
        user_id: UserId,
    ) -> UseCaseResult<Vec<CheckWithDiagnosticReport>> {
        let checks = self
            .check_repository
            .list_checks(limit, offset, user_id)
            .await?;

        let mut result = Vec::with_capacity(checks.len());
        for check in checks {
            let report = self
                .diagnostic_report_repository
                .find_by_check_id(check.id)
                .await?;
            result.push(CheckWithDiagnosticReport {
                check,
                diagnostic_report: report,
            });
        }

        Ok(result)
    }
}
