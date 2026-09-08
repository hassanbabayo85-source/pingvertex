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

        let claimed_rows = sqlx::query_as::<_, (Uuid, DateTime<Utc>, Uuid, bool)>(
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
            RETURNING m.id, m.lease_claimed_at, m.lease_execution_id,
                      (m.interval_seconds >= 300
                       AND (SELECT COUNT(*) FROM monitors m2 WHERE m2.tenant_id=m.tenant_id) <= 10
                       AND NOT EXISTS (
                           SELECT 1
                           FROM user_wallets w
                           WHERE w.tenant_id=m.tenant_id
                             AND w.payg_activated_at IS NOT NULL
                       ))
            "#,
        )
        .bind(limit_i64)
        .bind(lease_owner)
        .bind(lease_seconds)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        let mut monitors = Vec::with_capacity(claimed_rows.len());
        for (id, claimed_at, execution_id, free_entitled) in claimed_rows {
            if let Some(monitor) = self.monitor_repository.find_by_id(MonitorId(id)).await? {
                monitors.push(ClaimedMonitor {
                    monitor,
                    claimed_at,
                    execution_id,
                    free_entitled,
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
        let result=sqlx::query(r#"UPDATE monitors SET lease_owner=NULL,lease_expires_at=NULL WHERE lease_expires_at IS NOT NULL AND lease_expires_at < $1"#).bind(now).execute(&self.pool).await?;
        Ok(result.rows_affected())
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
        let rows=sqlx::query_as::<_,OneTimeRow>(r#"WITH selected AS (SELECT id FROM one_time_check_requests WHERE status='QUEUED' OR (status='RUNNING' AND lease_expires_at < $4) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1) UPDATE one_time_check_requests r SET status='RUNNING',lease_owner=$2,lease_expires_at=$4+($3::bigint*interval '1 second'),started_at=COALESCE(started_at,$4) FROM selected WHERE r.id=selected.id RETURNING r.id,r.tenant_id,r.monitor_id,r.created_by,r.request_id,r.status,r.execution_id,r.lease_owner,r.lease_expires_at,r.error,r.created_at,r.started_at,r.completed_at"#).bind(i64::from(l)).bind(o).bind(sec).bind(n).fetch_all(&self.pool).await?;
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
