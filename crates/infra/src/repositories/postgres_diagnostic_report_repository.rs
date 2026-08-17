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
    steps: Json<Vec<DiagnosticStep>>,
    root_cause_summary: String,
    created_at: DateTime<Utc>,
}

impl DiagnosticReportRow {
    fn from_domain(report: &DiagnosticReport) -> Self {
        Self {
            id: report.id.0,
            check_id: report.check_result_id.0,
            steps: Json(report.steps.clone()),
            root_cause_summary: report.root_cause_summary.clone(),
            created_at: report.created_at,
        }
    }

    fn into_domain(self) -> DiagnosticReport {
        DiagnosticReport {
            id: DiagnosticReportId(self.id),
            check_result_id: CheckId(self.check_id),
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

        // Kowanne check yana da diagnostic report guda daya kadai (duba
        // unique index a migration 0015). Idan aka sake kokarin ajiyewa
        // domin wannan check_id, mu sabunta maimakon mu gaza.
        sqlx::query(
            r#"
            INSERT INTO diagnostic_reports (
                id,
                check_id,
                steps,
                root_cause_summary,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (check_id) DO UPDATE SET
                steps = EXCLUDED.steps,
                root_cause_summary = EXCLUDED.root_cause_summary
            "#,
        )
        .bind(row.id)
        .bind(row.check_id)
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
}
