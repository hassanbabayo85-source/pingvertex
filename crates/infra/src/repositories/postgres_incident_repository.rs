use std::error::Error;

use application::ports::IncidentRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId, UserId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// B34: hard ceiling on `list_by_monitor`'s unfiltered result set. Same
/// reasoning as `PostgresCheckRepository::LIST_BY_MONITOR_LIMIT` — this
/// method is reachable (indirectly, via the public status handlers) from
/// unauthenticated endpoints and has no caller-supplied pagination. 200
/// most-recent-first incidents is generous headroom over what any current
/// caller actually consumes (the public response layer itself further
/// truncates to 10).
const LIST_BY_MONITOR_LIMIT: i64 = 200;

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
    last_seen_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    acknowledged_at: Option<DateTime<Utc>>,
    acknowledged_by: Option<Uuid>,
    occurred_during_maintenance: bool,
    escalation_step_index: i32,
    last_escalation_at: Option<DateTime<Utc>>,
}

impl IncidentRow {
    fn from_domain(incident: &Incident) -> Self {
        Self {
            id: incident.id.0,
            monitor_id: incident.monitor_id.0,
            started_at: incident.started_at,
            resolved_at: incident.resolved_at,
            last_seen_at: incident.last_seen_at,
            created_at: incident.created_at,
            updated_at: incident.updated_at,
            acknowledged_at: incident.acknowledged_at,
            acknowledged_by: incident.acknowledged_by.map(|uid| uid.0),
            occurred_during_maintenance: incident.occurred_during_maintenance,
            escalation_step_index: incident.escalation_step_index as i32,
            last_escalation_at: incident.last_escalation_at,
        }
    }

    fn into_domain(self) -> Incident {
        Incident {
            id: IncidentId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            started_at: self.started_at,
            resolved_at: self.resolved_at,
            last_seen_at: self.last_seen_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            acknowledged_at: self.acknowledged_at,
            acknowledged_by: self.acknowledged_by.map(UserId),
            occurred_during_maintenance: self.occurred_during_maintenance,
            escalation_step_index: self.escalation_step_index as u32,
            last_escalation_at: self.last_escalation_at,
        }
    }
}

const SELECT_COLUMNS: &str = "id, monitor_id, started_at, resolved_at, last_seen_at, created_at, updated_at, acknowledged_at, acknowledged_by, occurred_during_maintenance, escalation_step_index, last_escalation_at";

#[async_trait]
impl IncidentRepository for PostgresIncidentRepository {
    async fn save(&self, incident: &Incident) -> RepoResult<()> {
        let _ = self.save_and_report(incident).await?;
        Ok(())
    }

    async fn save_and_report(&self, incident: &Incident) -> RepoResult<bool> {
        let row = IncidentRow::from_domain(incident);

        let result = sqlx::query(
            r#"
            INSERT INTO incidents (
                id,
                monitor_id,
                started_at,
                resolved_at,
                last_seen_at,
                created_at,
                updated_at,
                acknowledged_at,
                acknowledged_by,
                occurred_during_maintenance,
                escalation_step_index,
                last_escalation_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (monitor_id) WHERE resolved_at IS NULL DO NOTHING
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.started_at)
        .bind(row.resolved_at)
        .bind(row.last_seen_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .bind(row.acknowledged_at)
        .bind(row.acknowledged_by)
        .bind(row.occurred_during_maintenance)
        .bind(row.escalation_step_index)
        .bind(row.last_escalation_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
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
                last_seen_at = $5,
                created_at = $6,
                updated_at = $7,
                acknowledged_at = $8,
                acknowledged_by = $9,
                occurred_during_maintenance = $10,
                escalation_step_index = $11,
                last_escalation_at = $12
            WHERE id = $1
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.started_at)
        .bind(row.resolved_at)
        .bind(row.last_seen_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .bind(row.acknowledged_at)
        .bind(row.acknowledged_by)
        .bind(row.occurred_during_maintenance)
        .bind(row.escalation_step_index)
        .bind(row.last_escalation_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn resolve_if_open(&self, incident: &Incident) -> RepoResult<bool> {
        let resolved_at = incident.resolved_at.unwrap_or_else(Utc::now);
        let updated_at = incident.updated_at;

        let result = sqlx::query(
            r#"
            UPDATE incidents
            SET resolved_at = $2, updated_at = $3
            WHERE id = $1 AND resolved_at IS NULL
            "#,
        )
        .bind(incident.id.0)
        .bind(resolved_at)
        .bind(updated_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn try_claim_escalation_step(
        &self,
        incident: &mut Incident,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let expected_step_index = incident.escalation_step_index as i32;

        let result = sqlx::query(
            r#"
            UPDATE incidents
            SET escalation_step_index = escalation_step_index + 1,
                last_escalation_at = $3,
                updated_at = $3
            WHERE id = $1
              AND escalation_step_index = $2
              AND resolved_at IS NULL
              AND acknowledged_at IS NULL
            "#,
        )
        .bind(incident.id.0)
        .bind(expected_step_index)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let claimed = result.rows_affected() > 0;
        if claimed {
            incident.escalation_step_index += 1;
            incident.last_escalation_at = Some(now);
            incident.updated_at = now;
        }

        Ok(claimed)
    }

    async fn find_by_id(&self, id: IncidentId) -> RepoResult<Option<Incident>> {
        let row = sqlx::query_as::<_, IncidentRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM incidents WHERE id = $1"
        ))
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(IncidentRow::into_domain))
    }

    async fn find_open_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Option<Incident>> {
        let row = sqlx::query_as::<_, IncidentRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM incidents \
             WHERE monitor_id = $1 AND resolved_at IS NULL \
             ORDER BY started_at DESC LIMIT 1"
        ))
        .bind(monitor_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(IncidentRow::into_domain))
    }

    /// B34: now bounded by `LIST_BY_MONITOR_LIMIT` — see the constant's
    /// doc comment.
    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Incident>> {
        let rows = sqlx::query_as::<_, IncidentRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM incidents \
             WHERE monitor_id = $1 ORDER BY started_at DESC LIMIT $2"
        ))
        .bind(monitor_id.0)
        .bind(LIST_BY_MONITOR_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(IncidentRow::into_domain).collect())
    }

    async fn list_open_unacknowledged(&self) -> RepoResult<Vec<Incident>> {
        let rows = sqlx::query_as::<_, IncidentRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM incidents \
             WHERE resolved_at IS NULL AND acknowledged_at IS NULL \
             ORDER BY started_at ASC"
        ))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(IncidentRow::into_domain).collect())
    }

    async fn touch_last_seen(&self, monitor_id: MonitorId, now: DateTime<Utc>) -> RepoResult<()> {
        sqlx::query(
            r#"
            UPDATE incidents
            SET last_seen_at = $2, updated_at = $2
            WHERE monitor_id = $1 AND resolved_at IS NULL
            "#,
        )
        .bind(monitor_id.0)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
