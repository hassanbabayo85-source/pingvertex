use std::time::Duration;

use application::ports::{AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome};
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use domain::RegionCode;
use domain::shared::ids::{CheckId, MonitorId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresAgentLeaseRepository {
    pool: PgPool,
}

impl PostgresAgentLeaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct LeaseStatusRow {
    agent_id: String,
    monitor_id: Uuid,
    region: String,
    completed_at: Option<DateTime<Utc>>,
    lease_expires_at: DateTime<Utc>,
}

#[async_trait]
impl AgentLeaseRepository for PostgresAgentLeaseRepository {
    async fn claim_due_checks(
        &self,
        agent_id: &str,
        region: &RegionCode,
        limit: u32,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<AgentLease>> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            DELETE FROM agent_check_leases
            WHERE completed_at IS NULL
              AND lease_expires_at <= $1
            "#,
        )
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let due_monitor_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT m.id
            FROM monitors m
            WHERE m.enabled = true
              AND m.kind != 'heartbeat'
              AND m.regions @> ARRAY[$1]::text[]
              AND NOT EXISTS (
                  SELECT 1
                  FROM agent_check_leases l
                  WHERE l.monitor_id = m.id
                    AND l.region = $1
                    AND l.completed_at IS NULL
              )
              AND COALESCE(
                  (
                      SELECT MAX(c.checked_at)
                      FROM checks c
                      WHERE c.monitor_id = m.id
                        AND c.region = $1
                  ),
                  'epoch'::timestamptz
              ) + (m.interval_seconds * interval '1 second') <= $2
            ORDER BY m.id
            FOR UPDATE SKIP LOCKED
            LIMIT $3
            "#,
        )
        .bind(region.as_str())
        .bind(now)
        .bind(limit as i64)
        .fetch_all(&mut *tx)
        .await?;

        let lease_expires_at = now + ChronoDuration::seconds(lease_duration.as_secs() as i64);
        let mut claimed = Vec::with_capacity(due_monitor_ids.len());

        for monitor_id in due_monitor_ids {
            let check_id = CheckId::new();

            sqlx::query(
                r#"
                INSERT INTO agent_check_leases (
                    check_id,
                    monitor_id,
                    region,
                    agent_id,
                    lease_expires_at,
                    claimed_at
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(check_id.0)
            .bind(monitor_id)
            .bind(region.as_str())
            .bind(agent_id)
            .bind(lease_expires_at)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            claimed.push(AgentLease {
                check_id,
                monitor_id: MonitorId(monitor_id),
                region: region.clone(),
                lease_expires_at,
            });
        }

        tx.commit().await?;
        Ok(claimed)
    }

    async fn submit_result(
        &self,
        agent_id: &str,
        check_id: CheckId,
        monitor_id: MonitorId,
        region: &RegionCode,
        now: DateTime<Utc>,
    ) -> RepoResult<SubmitAgentResultOutcome> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query(
            r#"
            UPDATE agent_check_leases
            SET completed_at = $4
            WHERE check_id = $1
              AND monitor_id = $2
              AND region = $3
              AND agent_id = $5
              AND completed_at IS NULL
              AND lease_expires_at > $4
            "#,
        )
        .bind(check_id.0)
        .bind(monitor_id.0)
        .bind(region.as_str())
        .bind(now)
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 1 {
            tx.commit().await?;
            return Ok(SubmitAgentResultOutcome::Accepted);
        }

        let row = sqlx::query_as::<_, LeaseStatusRow>(
            r#"
            SELECT agent_id, monitor_id, region, completed_at, lease_expires_at
            FROM agent_check_leases
            WHERE check_id = $1
            "#,
        )
        .bind(check_id.0)
        .fetch_optional(&mut *tx)
        .await?;

        let outcome = match row {
            None => SubmitAgentResultOutcome::Stale,
            Some(row) => {
                if row.completed_at.is_some() {
                    SubmitAgentResultOutcome::Duplicate
                } else if row.agent_id != agent_id
                    || row.monitor_id != monitor_id.0
                    || row.region != region.as_str()
                    || row.lease_expires_at <= now
                {
                    SubmitAgentResultOutcome::Stale
                } else {
                    SubmitAgentResultOutcome::Stale
                }
            }
        };

        tx.commit().await?;
        Ok(outcome)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM agent_check_leases
            WHERE completed_at IS NULL
              AND lease_expires_at <= $1
            "#,
        )
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
