use std::time::Duration;

use application::ports::{AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome};
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use domain::RegionCode;
use domain::shared::ids::{CheckId, MonitorId};
use domain::{Check, CheckStatus};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresAgentLeaseRepository {
    pool: PgPool,
}

impl PostgresAgentLeaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn insert_check(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        check: &Check,
    ) -> RepoResult<()> {
        let status: i16 = if matches!(check.status, CheckStatus::Up) {
            1
        } else {
            0
        };
        let error_type = format!("{:?}", check.error_type);

        sqlx::query(
            r#"
            INSERT INTO checks (
                id,
                monitor_id,
                region,
                status,
                http_status,
                response_time_ms,
                error_type,
                error_message,
                checked_at,
                cert_expires_at,
                cert_days_remaining,
                country_code
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (id, checked_at) DO NOTHING
            "#,
        )
        .bind(check.id.0)
        .bind(check.monitor_id.0)
        .bind(check.region.to_string())
        .bind(status)
        .bind(check.status_code.map(i32::from))
        .bind(check.latency_ms as i64)
        .bind(error_type)
        .bind(check.error_message.clone())
        .bind(check.checked_at)
        .bind(check.cert_expires_at)
        .bind(check.cert_days_remaining)
        .bind(check.country_code.clone())
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

#[derive(Debug, FromRow)]
struct LeaseStatusRow {
    completed_at: Option<DateTime<Utc>>,
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
            JOIN tenants t ON t.id = m.tenant_id
            WHERE m.enabled = true
              AND t.status = 'active'
              AND m.kind = 'http'
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
                    claimed_at,
                    billing_reserved
                )
                VALUES ($1, $2, $3, $4, $5, $6, FALSE)
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
                billing_reserved: false,
            });
        }

        tx.commit().await?;
        Ok(claimed)
    }

    async fn submit_result_with_check(
        &self,
        agent_id: &str,
        check_id: CheckId,
        monitor_id: MonitorId,
        region: &RegionCode,
        now: DateTime<Utc>,
        check: &Check,
    ) -> RepoResult<SubmitAgentResultOutcome> {
        let mut tx = self.pool.begin().await?;

        // Race Condition Check: A suspended tenant shouldn't record checks/billing
        let is_active: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT 1 FROM tenants t
            JOIN monitors m ON m.tenant_id = t.id
            WHERE m.id = $1 AND t.status = 'active'
            "#,
        )
        .bind(monitor_id.0)
        .fetch_optional(&mut *tx)
        .await?;

        if is_active.is_none() {
            // Drop silently to avoid payload billing
            tx.rollback().await?;
            return Ok(SubmitAgentResultOutcome::Stale);
        }

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
            self.insert_check(&mut tx, check).await?;
            tx.commit().await?;
            return Ok(SubmitAgentResultOutcome::Accepted);
        }

        let row = sqlx::query_as::<_, LeaseStatusRow>(
            r#"
            SELECT completed_at
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

    async fn mark_billing_reserved(&self, check_id: CheckId) -> RepoResult<()> {
        let affected = sqlx::query(
            "UPDATE agent_check_leases SET billing_reserved=TRUE WHERE check_id=$1 AND completed_at IS NULL",
        )
        .bind(check_id.0)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err("agent check lease is no longer active".into());
        }
        Ok(())
    }

    async fn billing_reserved(&self, check_id: CheckId) -> RepoResult<bool> {
        Ok(
            sqlx::query_scalar("SELECT billing_reserved FROM agent_check_leases WHERE check_id=$1")
                .bind(check_id.0)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or(false),
        )
    }
}
