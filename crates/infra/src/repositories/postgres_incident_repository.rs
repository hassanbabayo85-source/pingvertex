use std::error::Error;

use application::ports::IncidentRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresIncidentRepository {
    pool: PgPool,
}

impl PostgresIncidentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct IncidentRow {
    id: Uuid,
    monitor_id: Uuid,
    started_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl IncidentRow {
    fn from_domain(incident: &Incident) -> Self {
        Self {
            id: incident.id.0,
            monitor_id: incident.monitor_id.0,
            started_at: incident.started_at,
            resolved_at: incident.resolved_at,
            created_at: incident.created_at,
            updated_at: incident.updated_at,
        }
    }

    fn into_domain(self) -> Incident {
        Incident {
            id: IncidentId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            started_at: self.started_at,
            resolved_at: self.resolved_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[async_trait]
impl IncidentRepository for PostgresIncidentRepository {
    async fn save(&self, incident: &Incident) -> RepoResult<()> {
        let row = IncidentRow::from_domain(incident);

        sqlx::query(
            r#"
            INSERT INTO incidents (
                id,
                monitor_id,
                started_at,
                resolved_at,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.started_at)
        .bind(row.resolved_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, incident: &Incident) -> RepoResult<()> {
        let row = IncidentRow::from_domain(incident);

        sqlx::query(
            r#"
            UPDATE incidents
            SET
                monitor_id = $2,
                started_at = $3,
                resolved_at = $4,
                created_at = $5,
                updated_at = $6
            WHERE id = $1
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.started_at)
        .bind(row.resolved_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: IncidentId) -> RepoResult<Option<Incident>> {
        let row = sqlx::query_as::<_, IncidentRow>(
            r#"
            SELECT
                id,
                monitor_id,
                started_at,
                resolved_at,
                created_at,
                updated_at
            FROM incidents
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(IncidentRow::into_domain))
    }

    async fn find_open_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Option<Incident>> {
        let row = sqlx::query_as::<_, IncidentRow>(
            r#"
            SELECT
                id,
                monitor_id,
                started_at,
                resolved_at,
                created_at,
                updated_at
            FROM incidents
            WHERE monitor_id = $1
              AND resolved_at IS NULL
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(monitor_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(IncidentRow::into_domain))
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Incident>> {
        let rows = sqlx::query_as::<_, IncidentRow>(
            r#"
            SELECT
                id,
                monitor_id,
                started_at,
                resolved_at,
                created_at,
                updated_at
            FROM incidents
            WHERE monitor_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(monitor_id.0)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(IncidentRow::into_domain).collect())
    }
}
