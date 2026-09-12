use std::{error::Error, time::Duration};

use application::ports::{
    ClaimedMonitor, ClaimedOneTimeExecution, MonitorRepository, SchedulingRepository,
};
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

#[derive(sqlx::FromRow)]
struct OneTimeRow {
    id: Uuid,
    tenant_id: Uuid,
    monitor_id: Uuid,
    created_by: Uuid,
    request_id: String,
    status: String,
    execution_id: Uuid,
    lease_owner: Option<Uuid>,
    lease_expires_at: Option<DateTime<Utc>>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}
impl OneTimeRow {
    fn into_domain(self) -> domain::OneTimeCheckRequest {
        domain::OneTimeCheckRequest {
            id: self.id,
            tenant_id: self.tenant_id,
            monitor_id: self.monitor_id,
            created_by: self.created_by,
            request_id: self.request_id,
            status: self.status,
            execution_id: self.execution_id,
            lease_owner: self.lease_owner,
            lease_expires_at: self.lease_expires_at,
            error: self.error,
            created_at: self.created_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct OneTimeExecutionRow {
    id: Uuid,
    request_id: Uuid,
    tenant_id: Uuid,
    monitor_id: Uuid,
    region: String,
    execution_id: Uuid,
    status: String,
    lease_owner: Option<Uuid>,
    lease_expires_at: Option<DateTime<Utc>>,
    attempted: bool,
    billable: bool,
    charge_units: i64,
    error: Option<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}
impl OneTimeExecutionRow {
    fn into_domain(self) -> domain::OneTimeCheckExecution {
        domain::OneTimeCheckExecution {
            id: self.id,
            request_id: self.request_id,
            tenant_id: self.tenant_id,
            monitor_id: self.monitor_id,
            region: self.region,
            execution_id: self.execution_id,
            status: self.status,
            lease_owner: self.lease_owner,
            lease_expires_at: self.lease_expires_at,
            attempted: self.attempted,
            billable: self.billable,
            charge_units: self.charge_units,
            error: self.error,
            created_at: self.created_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
        }
    }
}

impl PostgresSchedulingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            monitor_repository: PostgresMonitorRepository::new(pool.clone()),
            pool,
        }
    }
}

impl PostgresSchedulingRepository {
    pub async fn renew_lease(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        extend_by: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        <Self as SchedulingRepository>::renew_lease(self, monitor_id, lease_owner, extend_by, now)
            .await
    }
}

impl PostgresSchedulingRepository {
    async fn claim_due_monitors_inner(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
        max_concurrency_per_tenant: Option<usize>,
        due_claim_limit_per_tenant: Option<usize>,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        let lease_seconds = i64::try_from(lease_duration.as_secs()).unwrap_or(i64::MAX);
        let limit_i64 = i64::from(limit);
        let max_per_tenant = i64::try_from(max_concurrency_per_tenant.unwrap_or(i64::MAX as usize))
            .unwrap_or(i64::MAX);
        let due_per_tenant = i64::try_from(due_claim_limit_per_tenant.unwrap_or(i64::MAX as usize))
            .unwrap_or(i64::MAX);

        let query = r#"
            WITH selected AS (
                SELECT m.id
                FROM monitors m
                JOIN tenants t ON t.id = m.tenant_id
                WHERE m.enabled = true
                  AND t.status = 'active'
                  AND m.kind <> 'heartbeat'
                  AND (m.kind <> 'http' OR cardinality(m.regions) <= 1)
                  AND m.lease_expires_at IS NULL
                  AND m.next_due_at <= $4
                  AND NOT EXISTS (
                      SELECT 1
                      FROM monitor_billing_blocks b
                      WHERE b.monitor_id = m.id
                  )
                  AND (
                      $5 = 9223372036854775807
                      OR (
                          pg_try_advisory_xact_lock(
                              hashtextextended('scheduler:' || m.tenant_id::text, 0)
                          )
                          AND (
                              SELECT COUNT(*)
                              FROM monitors leased
                              WHERE leased.tenant_id = m.tenant_id
                                AND leased.lease_owner IS NOT NULL
                                AND leased.lease_expires_at > $4
                          ) < $5
                          AND (
                              SELECT COUNT(*)
                              FROM monitors earlier
                              WHERE earlier.tenant_id = m.tenant_id
                                AND earlier.enabled = true
                                AND earlier.kind <> 'heartbeat'
                                AND (earlier.kind <> 'http' OR cardinality(earlier.regions) <= 1)
                                AND earlier.lease_expires_at IS NULL
                                AND earlier.next_due_at <= $4
                                AND NOT EXISTS (
                                    SELECT 1 FROM monitor_billing_blocks eb
                                    WHERE eb.monitor_id = earlier.id
                                )
                                AND (
                                    earlier.next_due_at < m.next_due_at
                                    OR (
                                        earlier.next_due_at = m.next_due_at
                                        AND earlier.id <= m.id
                                    )
                                )
                          ) <= LEAST(
                              $6,
                              $5 - (
                                  SELECT COUNT(*)
                                  FROM monitors leased_now
                                  WHERE leased_now.tenant_id = m.tenant_id
                                    AND leased_now.lease_owner IS NOT NULL
                                    AND leased_now.lease_expires_at > $4
                              )
                          )
                      )
                  )
                ORDER BY m.next_due_at, m.id
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
            RETURNING m.id, m.lease_claimed_at, m.next_due_at, m.lease_execution_id
        "#;

        let claimed_rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, DateTime<Utc>, Uuid)>(query)
            .bind(limit_i64)
            .bind(lease_owner)
            .bind(lease_seconds)
            .bind(now)
            .bind(max_per_tenant)
            .bind(due_per_tenant)
            .fetch_all(&self.pool)
            .await?;

        let mut monitors = Vec::with_capacity(claimed_rows.len());
        for (id, claimed_at, scheduled_at, execution_id) in claimed_rows {
            if let Some(monitor) = self.monitor_repository.find_by_id(MonitorId(id)).await? {
                monitors.push(ClaimedMonitor {
                    monitor,
                    claimed_at,
                    scheduled_at,
                    execution_id,
                });
            }
        }

        Ok(monitors)
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
        self.claim_due_monitors_inner(limit, lease_owner, lease_duration, now, None, None)
            .await
    }

    async fn claim_due_monitors_with_limits(
        &self,
        limit: u32,
        lease_owner: Uuid,
        lease_duration: Duration,
        now: DateTime<Utc>,
        max_concurrency_per_tenant: usize,
        due_claim_limit_per_tenant: usize,
    ) -> RepoResult<Vec<ClaimedMonitor>> {
        self.claim_due_monitors_inner(
            limit,
            lease_owner,
            lease_duration,
            now,
            Some(max_concurrency_per_tenant.max(1)),
            Some(due_claim_limit_per_tenant.max(1)),
        )
        .await
    }

    async fn due_monitor_count(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM monitors m
            JOIN tenants t ON t.id = m.tenant_id
            WHERE m.enabled = true
              AND t.status = 'active'
              AND m.kind <> 'heartbeat'
              AND (m.kind <> 'http' OR cardinality(m.regions) <= 1)
              AND m.lease_expires_at IS NULL
              AND m.next_due_at <= $1
              AND NOT EXISTS (
                  SELECT 1 FROM monitor_billing_blocks b
                  WHERE b.monitor_id = m.id
              )
            "#,
        )
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.max(0) as u64)
    }

    async fn complete_claim(&self, monitor_id: MonitorId, lease_owner: Uuid) -> RepoResult<()> {
        self.complete_claim_with_spread(monitor_id, lease_owner, 0)
            .await
    }

    async fn complete_claim_with_spread(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        spread_seconds: u64,
    ) -> RepoResult<()> {
        let spread = i64::try_from(spread_seconds.min(300)).unwrap_or(300);
        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET lease_owner = NULL,
                lease_expires_at = NULL,
                lease_claimed_at = NULL,
                lease_execution_id = NULL,
                next_due_at = now()
                    + (interval_seconds * interval '1 second')
                    + CASE
                        WHEN $3 > 0 THEN (
                            mod(
                                hashtextextended(id::text, 0) & 9223372036854775807,
                                $3
                            ) * interval '1 second'
                        )
                        ELSE interval '0 second'
                      END
            WHERE id = $1
              AND lease_owner = $2
            "#,
        )
        .bind(monitor_id.0)
        .bind(lease_owner)
        .bind(spread)
        .execute(&self.pool)
        .await?;

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

    async fn renew_lease(
        &self,
        monitor_id: MonitorId,
        lease_owner: Uuid,
        extend_by: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let extend_seconds = i64::try_from(extend_by.as_secs()).unwrap_or(i64::MAX);

        let result = sqlx::query(
            r#"
            UPDATE monitors
            SET lease_expires_at = $3 + ($4 * interval '1 second'),
                updated_at = now()
            WHERE id = $1
              AND lease_owner = $2
              AND lease_expires_at IS NOT NULL
              AND lease_expires_at > $3
            "#,
        )
        .bind(monitor_id.0)
        .bind(lease_owner)
        .bind(now)
        .bind(extend_seconds)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: domain::shared::ids::TenantId,
        reason: &str,
    ) -> RepoResult<bool> {
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 500 {
            return Err("invalid billing block reason".into());
        }
        let result = sqlx::query(
            r#"
            INSERT INTO monitor_billing_blocks(monitor_id, tenant_id, reason)
            SELECT id, tenant_id, $3
            FROM monitors
            WHERE id = $1 AND tenant_id = $2
            ON CONFLICT (monitor_id) DO UPDATE
            SET reason = EXCLUDED.reason, updated_at = now()
            "#,
        )
        .bind(monitor_id.0)
        .bind(tenant_id.0)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn enqueue_one_time_check(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        u: domain::shared::ids::UserId,
        r: &str,
        regions: &[domain::RegionCode],
    ) -> RepoResult<domain::OneTimeCheckRequest> {
        let mut tx = self.pool.begin().await?;

        let request_row = if let Some(existing) = sqlx::query_as::<_, OneTimeRow>(
            r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at
               FROM one_time_check_requests
               WHERE tenant_id=$1 AND request_id=$2"#,
        )
        .bind(t.0)
        .bind(r)
        .fetch_optional(&mut *tx)
        .await?
        {
            if existing.monitor_id != m.0 {
                return Err("one-time idempotency key is already used for another monitor".into());
            }
            existing
        } else {
            sqlx::query_as::<_, OneTimeRow>(
                r#"INSERT INTO one_time_check_requests
                     (id,tenant_id,monitor_id,created_by,request_id,status,execution_id)
                   VALUES($1,$2,$3,$4,$5,'QUEUED',$6)
                   RETURNING id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at"#,
            )
            .bind(Uuid::new_v4())
            .bind(t.0)
            .bind(m.0)
            .bind(u.0)
            .bind(r)
            .bind(Uuid::new_v4())
            .fetch_one(&mut *tx)
            .await?
        };

        // Fan out one execution per region.
        for region in regions {
            sqlx::query(
                r#"INSERT INTO one_time_check_executions
                     (id, request_id, tenant_id, monitor_id, region, execution_id, status)
                   VALUES ($1, $2, $3, $4, $5, $6, 'QUEUED')
                   ON CONFLICT (request_id, region) DO NOTHING"#,
            )
            .bind(Uuid::new_v4())
            .bind(request_row.id)
            .bind(t.0)
            .bind(m.0)
            .bind(region.as_str())
            .bind(Uuid::new_v4())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(request_row.into_domain())
    }

    async fn claim_one_time_executions(
        &self,
        l: u32,
        o: Uuid,
        d: Duration,
        n: DateTime<Utc>,
    ) -> RepoResult<Vec<ClaimedOneTimeExecution>> {
        let sec = d.as_secs() as i64;
        let rows = sqlx::query_as::<_, OneTimeExecutionRow>(
            r#"
            WITH selected AS (
                SELECT e.id
                FROM one_time_check_executions e
                JOIN tenants t
                  ON t.id = e.tenant_id
                JOIN monitors m
                  ON m.id = e.monitor_id
                 AND m.tenant_id = e.tenant_id
                WHERE t.status = 'active'
                  AND m.enabled = true
                  AND (
                      e.status = 'QUEUED'
                      OR (e.status = 'RUNNING' AND e.lease_expires_at < $4)
                  )
                ORDER BY e.created_at
                FOR UPDATE OF e SKIP LOCKED
                LIMIT $1
            )
            UPDATE one_time_check_executions e
            SET status = 'RUNNING',
                lease_owner = $2,
                lease_expires_at = $4 + ($3::bigint * interval '1 second'),
                started_at = COALESCE(e.started_at, $4)
            FROM selected
            WHERE e.id = selected.id
            RETURNING e.id, e.request_id, e.tenant_id, e.monitor_id, e.region,
                      e.execution_id, e.status, e.lease_owner, e.lease_expires_at,
                      e.attempted, e.billable, e.charge_units, e.error,
                      e.created_at, e.started_at, e.completed_at
            "#,
        )
        .bind(i64::from(l))
        .bind(o)
        .bind(sec)
        .bind(n)
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let monitor = self
                .monitor_repository
                .find_by_id(MonitorId(row.monitor_id))
                .await?;
            let request = sqlx::query_as::<_, OneTimeRow>(
                r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at
                   FROM one_time_check_requests WHERE id=$1"#,
            )
            .bind(row.request_id)
            .fetch_one(&self.pool)
            .await?;

            if let Some(monitor) = monitor {
                out.push(ClaimedOneTimeExecution {
                    execution: row.into_domain(),
                    request: request.into_domain(),
                    monitor,
                });
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_one_time_execution(
        &self,
        execution_id: Uuid,
        lease_owner: Uuid,
        status: &str,
        error: Option<&str>,
        attempted: bool,
        billable: bool,
        charge_units: i64,
    ) -> RepoResult<()> {
        if !matches!(
            status,
            domain::OneTimeCheckExecution::SUCCEEDED | domain::OneTimeCheckExecution::FAILED
        ) {
            return Err("invalid one-time execution terminal status".into());
        }

        let mut tx = self.pool.begin().await?;

        let n = sqlx::query(
            r#"UPDATE one_time_check_executions
               SET status=$3, error=$4, completed_at=now(),
                   lease_owner=NULL, lease_expires_at=NULL,
                   attempted=$5, billable=$6, charge_units=$7
               WHERE id=$1 AND lease_owner=$2 AND status='RUNNING'"#,
        )
        .bind(execution_id)
        .bind(lease_owner)
        .bind(status)
        .bind(error)
        .bind(attempted)
        .bind(billable)
        .bind(charge_units)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if n != 1 {
            return Err("one-time execution claim was lost before completion".into());
        }

        let request_id: Uuid =
            sqlx::query_scalar("SELECT request_id FROM one_time_check_executions WHERE id=$1")
                .bind(execution_id)
                .fetch_one(&mut *tx)
                .await?;

        sqlx::query(
            r#"
            UPDATE one_time_check_requests r
            SET status = CASE
                    WHEN EXISTS (SELECT 1 FROM one_time_check_executions e
                                 WHERE e.request_id = r.id
                                   AND e.status IN ('QUEUED','RUNNING'))
                        THEN 'RUNNING'
                    WHEN EXISTS (SELECT 1 FROM one_time_check_executions e
                                 WHERE e.request_id = r.id
                                   AND e.status = 'SUCCEEDED')
                        THEN 'SUCCEEDED'
                    ELSE 'FAILED'
                END,
                completed_at = CASE
                    WHEN NOT EXISTS (SELECT 1 FROM one_time_check_executions e
                                     WHERE e.request_id = r.id
                                       AND e.status IN ('QUEUED','RUNNING'))
                        THEN now()
                    ELSE r.completed_at
                END
            WHERE r.id = $1
            "#,
        )
        .bind(request_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_one_time_check_executions(
        &self,
        request_id: Uuid,
    ) -> RepoResult<Vec<domain::OneTimeCheckExecution>> {
        let rows = sqlx::query_as::<_, OneTimeExecutionRow>(
            r#"SELECT id, request_id, tenant_id, monitor_id, region, execution_id, status,
                      lease_owner, lease_expires_at, attempted, billable, charge_units, error,
                      created_at, started_at, completed_at
               FROM one_time_check_executions
               WHERE request_id = $1
               ORDER BY region ASC"#,
        )
        .bind(request_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(OneTimeExecutionRow::into_domain)
            .collect())
    }

    async fn get_one_time_check(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        r: Uuid,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>> {
        let row = sqlx::query_as::<_, OneTimeRow>(
            r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at
               FROM one_time_check_requests WHERE id=$1 AND tenant_id=$2 AND monitor_id=$3"#,
        )
        .bind(r)
        .bind(t.0)
        .bind(m.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(OneTimeRow::into_domain))
    }

    async fn get_one_time_check_by_request_id(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        r: &str,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>> {
        let row = sqlx::query_as::<_, OneTimeRow>(
            r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at
               FROM one_time_check_requests
               WHERE tenant_id=$1 AND monitor_id=$2 AND request_id=$3"#,
        )
        .bind(t.0)
        .bind(m.0)
        .bind(r)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(OneTimeRow::into_domain))
    }
}
