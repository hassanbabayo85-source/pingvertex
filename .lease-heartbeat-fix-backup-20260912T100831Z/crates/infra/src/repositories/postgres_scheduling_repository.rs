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
impl PostgresSchedulingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self {
            monitor_repository: PostgresMonitorRepository::new(pool.clone()),
            pool,
        }
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
            RETURNING m.id, m.lease_claimed_at, m.lease_execution_id
        "#;

        let claimed_rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, Uuid)>(query)
            .bind(limit_i64)
            .bind(lease_owner)
            .bind(lease_seconds)
            .bind(now)
            .bind(max_per_tenant)
            .bind(due_per_tenant)
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
        // Recover only the current lease. Keep lease_execution_id intact so
        // PAYG idempotency survives a worker crash and a subsequent reclaim.
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
    ) -> RepoResult<domain::OneTimeCheckRequest> {
        let mut tx = self.pool.begin().await?;

        if let Some(existing) = sqlx::query_as::<_, OneTimeRow>(
            r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at
               FROM one_time_check_requests
               WHERE tenant_id=$1 AND monitor_id=$2 AND request_id=$3"#,
        )
        .bind(t.0)
        .bind(m.0)
        .bind(r)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(existing.into_domain());
        }

        let row = sqlx::query_as::<_, OneTimeRow>(
            r#"INSERT INTO one_time_check_requests
                 (id,tenant_id,monitor_id,created_by,request_id,status,execution_id)
               VALUES($1,$2,$3,$4,$5,'QUEUED',$6)
               ON CONFLICT (tenant_id,request_id) DO NOTHING
               RETURNING id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at"#,
        )
        .bind(Uuid::new_v4())
        .bind(t.0)
        .bind(m.0)
        .bind(u.0)
        .bind(r)
        .bind(Uuid::new_v4())
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = row {
            tx.commit().await?;
            return Ok(row.into_domain());
        }

        let existing = sqlx::query_as::<_, OneTimeRow>(
            r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at
               FROM one_time_check_requests
               WHERE tenant_id=$1 AND request_id=$2"#,
        )
        .bind(t.0)
        .bind(r)
        .fetch_one(&mut *tx)
        .await?;

        if existing.monitor_id != m.0 {
            return Err("one-time idempotency key is already used for another monitor".into());
        }

        tx.commit().await?;
        Ok(existing.into_domain())
    }
    async fn claim_one_time_checks(
        &self,
        l: u32,
        o: Uuid,
        d: Duration,
        n: DateTime<Utc>,
    ) -> RepoResult<Vec<application::ports::ClaimedOneTimeCheck>> {
        let sec = d.as_secs() as i64;
        let rows = sqlx::query_as::<_,OneTimeRow>(
            r#"
            WITH selected AS (
                SELECT r.id
                FROM one_time_check_requests r
                JOIN tenants t
                  ON t.id = r.tenant_id
                JOIN monitors m
                  ON m.id = r.monitor_id
                 AND m.tenant_id = r.tenant_id
                WHERE t.status = 'active'
                  AND m.enabled = true
                  AND (
                      r.status = 'QUEUED'
                      OR (r.status = 'RUNNING' AND r.lease_expires_at < $4)
                  )
                ORDER BY r.created_at
                FOR UPDATE OF r SKIP LOCKED 
                LIMIT $1
            ) 
            UPDATE one_time_check_requests r 
            SET status='RUNNING',lease_owner=$2,lease_expires_at=$4+($3::bigint*interval '1 second'),started_at=COALESCE(started_at,$4) 
            FROM selected 
            WHERE r.id=selected.id 
            RETURNING r.id,r.tenant_id,r.monitor_id,r.created_by,r.request_id,r.status,r.execution_id,r.lease_owner,r.lease_expires_at,r.error,r.created_at,r.started_at,r.completed_at
            "#)
            .bind(i64::from(l))
            .bind(o)
            .bind(sec)
            .bind(n)
            .fetch_all(&self.pool)
            .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(monitor) = self
                .monitor_repository
                .find_by_id(MonitorId(row.monitor_id))
                .await?
            {
                out.push(application::ports::ClaimedOneTimeCheck {
                    request: row.into_domain(),
                    monitor,
                });
            }
        }
        Ok(out)
    }
    async fn complete_one_time_check(
        &self,
        r: Uuid,
        o: Uuid,
        s: &str,
        e: Option<&str>,
    ) -> RepoResult<()> {
        if !matches!(
            s,
            domain::OneTimeCheckRequest::SUCCEEDED | domain::OneTimeCheckRequest::FAILED
        ) {
            return Err("invalid one-time check terminal status".into());
        }
        let n=sqlx::query(r#"UPDATE one_time_check_requests SET status=$3,error=$4,completed_at=now(),lease_owner=NULL,lease_expires_at=NULL WHERE id=$1 AND lease_owner=$2 AND status='RUNNING'"#).bind(r).bind(o).bind(s).bind(e).execute(&self.pool).await?.rows_affected();
        if n != 1 {
            return Err("one-time check claim was lost before completion".into());
        }
        Ok(())
    }
    async fn get_one_time_check(
        &self,
        t: domain::shared::ids::TenantId,
        m: MonitorId,
        r: Uuid,
    ) -> RepoResult<Option<domain::OneTimeCheckRequest>> {
        let row=sqlx::query_as::<_,OneTimeRow>(r#"SELECT id,tenant_id,monitor_id,created_by,request_id,status,execution_id,lease_owner,lease_expires_at,error,created_at,started_at,completed_at FROM one_time_check_requests WHERE id=$1 AND tenant_id=$2 AND monitor_id=$3"#).bind(r).bind(t.0).bind(m.0).fetch_optional(&self.pool).await?;
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
