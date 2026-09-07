use std::{error::Error, time::Duration};

use application::ports::{ClaimedMonitor, MonitorRepository, SchedulingRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::MonitorId;
use sqlx::PgPool;
use uuid::Uuid;

use super::postgres_monitor_repository::PostgresMonitorRepository;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct PostgresSchedulingRepository {
    pool: PgPool,
    monitor_repository: PostgresMonitorRepository,
}

impl PostgresSchedulingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            monitor_repository: PostgresMonitorRepository::new(pool.clone()),
            pool,
        }
    }
}

#[async_trait]
impl SchedulingRepository for PostgresSchedulingRepository {
    async fn claim_due_monitors(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        let lease_seconds = lease_duration.as_secs() as i64;
        let limit_i64 = i64::from(limit);

        let claimed_rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, Uuid)>(
            r#"
            WITH selected AS (
                SELECT id
                FROM monitors
                WHERE enabled = true
                  AND kind != 'heartbeat'
                  AND lease_expires_at IS NULL
                  AND next_due_at <= $4
                ORDER BY next_due_at
                FOR UPDATE SKIP LOCKED
                LIMIT $1
            )
            UPDATE monitors m
            SET lease_owner = $2,
                lease_expires_at = $4 + ($3::bigint * interval '1 second'),
                lease_claimed_at = COALESCE(m.lease_claimed_at, $4),
                lease_execution_id = COALESCE(m.lease_execution_id, gen_random_uuid())
            FROM selected
            WHERE m.id = selected.id
            RETURNING m.id, m.lease_claimed_at, m.lease_execution_id
            "#,
        )
        .bind(limit_i64)
        .bind(lease_owner)
        .bind(lease_seconds)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        let mut monitors = Vec::with_capacity(claimed_rows.len());
        for (id, claimed_at, execution_id) in claimed_rows {
            if let Some(monitor) = self.monitor_repository.find_by_id(MonitorId(id)).await? {
                monitors.push(ClaimedMonitor {
                    monitor,
                    claimed_at,
                    execution_id,
                });
            }
        }

        Ok(monitors)
    }

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET lease_owner = NULL,
                lease_expires_at = NULL,
                lease_claimed_at = NULL,
                lease_execution_id = NULL,
                next_due_at = now() + (interval_seconds * interval '1 second')
            WHERE id = $1
              AND lease_owner = $2
            "#,
        )
        .bind(monitor_id.0)
        .bind(lease_owner)
        .execute(&self.pool)
        .await?;

        // A zero-row update means this worker no longer owns the claim (for
        // example, the lease expired and another worker reclaimed it). Do not
        // clear or advance another worker's scheduling state.
        if result.rows_affected() == 0 {
            return Err("scheduler claim was lost before completion".into());
        }

        Ok(())
    }

    async fn request_immediate_checks(
        &self,
        tenant_id: domain::shared::ids::TenantId,
    ) -> RepoResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET next_due_at = LEAST(next_due_at, NOW())
            WHERE tenant_id = $1
              AND enabled = TRUE
              AND kind <> 'heartbeat'
            "#,
        )
        .bind(tenant_id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET lease_owner = NULL,
                lease_expires_at = NULL
            WHERE lease_expires_at IS NOT NULL
              AND lease_expires_at < $1
            "#,
        )
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
