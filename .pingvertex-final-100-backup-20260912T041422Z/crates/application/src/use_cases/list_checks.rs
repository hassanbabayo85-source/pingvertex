use std::{error::Error, sync::Arc};

use crate::dto::CheckWithDiagnosticReport;
use crate::ports::check_repository::CheckFilters;
use crate::ports::{CheckRepository, DiagnosticReportRepository};
use domain::shared::ids::TenantId;
use domain::shared::types::AuthenticatedUser;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
        authenticated_user: AuthenticatedUser,
        filters: CheckFilters,
    ) -> UseCaseResult<Vec<CheckWithDiagnosticReport>> {
        self.execute_for_tenant(
            limit,
            offset,
            authenticated_user.tenant_context().tenant_id(),
            filters,
        )
        .await
    }

    /// Tenant-scoped variant of `execute` that takes a `TenantId`
    /// directly instead of an `AuthenticatedUser`.
    ///
    /// Added for the API-key-authenticated public surface
    /// (`GET /api/v1/external/checks`, B31), where the caller is
    /// identified by `middleware::api_key_auth::ApiKeyPrincipal` — which
    /// carries a bare `tenant_id`/`user_id`, not a `Role` — rather than a
    /// session-derived `AuthenticatedUser`. Synthesizing an
    /// `AuthenticatedUser` from an API key principal is deliberately
    /// avoided elsewhere in this codebase (see the doc comment on
    /// `ApiKeyPrincipal`), so this method exists instead of forcing
    /// callers to fabricate one. `execute` above is now a thin wrapper
    /// around this.
    pub async fn execute_for_tenant(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
        filters: CheckFilters,
    ) -> UseCaseResult<Vec<CheckWithDiagnosticReport>> {
        let checks = self
            .check_repository
            .list_checks_filtered(limit, offset, tenant_id, filters)
            .await?;

        // B35: load all diagnostic reports in one batch, not one-by-one.
        let check_ids: Vec<_> = checks.iter().map(|check| check.id).collect();
        let diagnostic_reports = self
            .diagnostic_report_repository
            .find_by_check_ids(&check_ids)
            .await?;

        let mut result = Vec::with_capacity(checks.len());
        for check in checks {
            let diagnostic_report = diagnostic_reports.get(&check.id).cloned();
            result.push(CheckWithDiagnosticReport {
                check,
                diagnostic_report,
            });
        }

        Ok(result)
    }
}
