use std::collections::HashMap;
use std::error::Error;

use application::ports::DiagnosticReportRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{CheckId, DiagnosticReportId};
use domain::{DiagnosticReport, DiagnosticStep};
use sqlx::types::Json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresDiagnosticReportRepository {
    pool: PgPool,
}

impl PostgresDiagnosticReportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct DiagnosticReportRow {
    id: Uuid,
    check_id: Uuid,
    check_checked_at: DateTime<Utc>,
    steps: Json<Vec<DiagnosticStep>>,
    root_cause_summary: String,
    created_at: DateTime<Utc>,
}

impl DiagnosticReportRow {
    fn from_domain(report: &DiagnosticReport) -> Self {
        Self {
            id: report.id.0,
            check_id: report.check_result_id.0,
            check_checked_at: report.check_checked_at,
            steps: Json(report.steps.clone()),
            root_cause_summary: report.root_cause_summary.clone(),
            created_at: report.created_at,
        }
    }

    fn into_domain(self) -> DiagnosticReport {
        DiagnosticReport {
            id: DiagnosticReportId(self.id),
            check_result_id: CheckId(self.check_id),
            check_checked_at: self.check_checked_at,
            steps: self.steps.0,
            root_cause_summary: self.root_cause_summary,
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl DiagnosticReportRepository for PostgresDiagnosticReportRepository {
    async fn save(&self, report: &DiagnosticReport) -> RepoResult<()> {
        let row = DiagnosticReportRow::from_domain(report);

        // Each check has exactly one diagnostic report (see the unique index
        // in migration 0015). If a save is retried for the same check_id, we
        // update instead of failing.
        sqlx::query(
            r#"
            INSERT INTO diagnostic_reports (
                id,
                check_id,
                check_checked_at,
                steps,
                root_cause_summary,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (check_id) DO UPDATE SET
                steps = EXCLUDED.steps,
                root_cause_summary = EXCLUDED.root_cause_summary
            "#,
        )
        .bind(row.id)
        .bind(row.check_id)
        .bind(row.check_checked_at)
        .bind(&row.steps)
        .bind(&row.root_cause_summary)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_check_id(&self, check_id: CheckId) -> RepoResult<Option<DiagnosticReport>> {
        let row = sqlx::query_as::<_, DiagnosticReportRow>(
            r#"
            SELECT
                id,
                check_id,
                check_checked_at,
                steps,
                root_cause_summary,
                created_at
            FROM diagnostic_reports
            WHERE check_id = $1
            "#,
        )
        .bind(check_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(DiagnosticReportRow::into_domain))
    }

    /// Single-batch load for a page of checks. Replaces the default
    /// one-query-per-check loop and removes the N+1 hit on
    /// `GET /api/v1/checks` and `GET /api/v1/monitors/{id}/checks`.
    async fn find_by_check_ids(
        &self,
        check_ids: &[CheckId],
    ) -> RepoResult<HashMap<CheckId, DiagnosticReport>> {
        if check_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let ids: Vec<Uuid> = check_ids.iter().map(|id| id.0).collect();

        let rows = sqlx::query_as::<_, DiagnosticReportRow>(
            r#"
            SELECT
                id,
                check_id,
                check_checked_at,
                steps,
                root_cause_summary,
                created_at
            FROM diagnostic_reports
            WHERE check_id = ANY($1)
            "#,
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;

        let mut reports = HashMap::with_capacity(rows.len());
        for row in rows {
            let report = row.into_domain();
            reports.insert(report.check_result_id, report);
        }

        Ok(reports)
    }
}
