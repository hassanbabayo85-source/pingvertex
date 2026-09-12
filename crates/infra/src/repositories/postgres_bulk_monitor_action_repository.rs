use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use application::ports::{
    BulkMonitorActionJob, BulkMonitorActionRepoResult, BulkMonitorActionRepository,
};

pub struct PostgresBulkMonitorActionRepository {
    pool: PgPool,
}

impl PostgresBulkMonitorActionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_job(
        row: &sqlx::postgres::PgRow,
    ) -> BulkMonitorActionRepoResult<BulkMonitorActionJob> {
        use sqlx::Row;
        let ids: Option<Vec<Uuid>> = row.try_get("monitor_ids")?;
        Ok(BulkMonitorActionJob {
            id: row.try_get("id")?,
            tenant_id: row.try_get("tenant_id")?,
            actor_user_id: row.try_get("actor_user_id")?,
            action: row.try_get("action")?,
            monitor_ids: ids,
            all: row.try_get("all")?,
            reason: row.try_get("reason")?,
            idempotency_key: row.try_get("idempotency_key")?,
            status: row.try_get("status")?,
            affected: row.try_get::<Option<i64>, _>("affected")?.map(|v| v as u64),
            error: row.try_get("error")?,
        })
    }
}

#[async_trait]
impl BulkMonitorActionRepository for PostgresBulkMonitorActionRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        actor_user_id: Uuid,
        action: &str,
        monitor_ids: Option<&[Uuid]>,
        all: bool,
        reason: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> BulkMonitorActionRepoResult<BulkMonitorActionJob> {
        let id = Uuid::new_v4();
        let row = sqlx::query(
            "INSERT INTO monitor_bulk_action_jobs (id, tenant_id, actor_user_id, action, monitor_ids, \"all\", reason, idempotency_key, status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'PENDING') ON CONFLICT (tenant_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING RETURNING *"
        )
        .bind(id).bind(tenant_id).bind(actor_user_id).bind(action).bind(monitor_ids.map(|v| v.to_vec())).bind(all).bind(reason).bind(idempotency_key)
        .fetch_optional(&self.pool).await?;
        if let Some(row) = row {
            return Self::row_to_job(&row);
        }
        let existing = sqlx::query(
            "SELECT * FROM monitor_bulk_action_jobs WHERE tenant_id=$1 AND idempotency_key=$2",
        )
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_one(&self.pool)
        .await?;
        let job = Self::row_to_job(&existing)?;
        let incoming_ids = monitor_ids.map(|v| v.to_vec());
        if job.action != action
            || job.all != all
            || job.monitor_ids != incoming_ids
            || job.reason.as_deref() != reason
        {
            return Err(
                "Idempotency-Key was already used for a different bulk monitor action".into(),
            );
        }
        Ok(job)
    }

    async fn find_for_tenant(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorActionRepoResult<Option<BulkMonitorActionJob>> {
        let row =
            sqlx::query("SELECT * FROM monitor_bulk_action_jobs WHERE id=$1 AND tenant_id=$2")
                .bind(job_id)
                .bind(tenant_id)
                .fetch_optional(&self.pool)
                .await?;
        row.as_ref().map(Self::row_to_job).transpose()
    }

    async fn queue_depth(&self) -> BulkMonitorActionRepoResult<u64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM monitor_bulk_action_jobs WHERE status='PENDING' OR (status='RUNNING' AND lease_expires_at <= now())",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn claim_jobs(
        &self,
        worker_id: Uuid,
        limit: u32,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorActionRepoResult<Vec<BulkMonitorActionJob>> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT * FROM monitor_bulk_action_jobs WHERE status='PENDING' OR (status='RUNNING' AND lease_expires_at <= now()) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT $1"
        ).bind(limit as i64).fetch_all(&mut *tx).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = sqlx::Row::try_get(&row, "id")?;
            sqlx::query("UPDATE monitor_bulk_action_jobs SET status='RUNNING', worker_id=$2, lease_expires_at=$3, updated_at=now(), attempts=attempts+1 WHERE id=$1")
                .bind(id).bind(worker_id).bind(lease_until).execute(&mut *tx).await?;
            out.push(Self::row_to_job(&row)?);
        }
        tx.commit().await?;
        Ok(out)
    }

    async fn complete(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        affected: u64,
    ) -> BulkMonitorActionRepoResult<bool> {
        let result = sqlx::query("UPDATE monitor_bulk_action_jobs SET status='COMPLETED', affected=$3, worker_id=NULL, lease_expires_at=NULL, updated_at=now() WHERE id=$1 AND worker_id=$2 AND status='RUNNING'")
            .bind(job_id).bind(worker_id).bind(i64::try_from(affected).unwrap_or(i64::MAX)).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }

    async fn fail(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        error: &str,
    ) -> BulkMonitorActionRepoResult<bool> {
        let result = sqlx::query("UPDATE monitor_bulk_action_jobs SET status='FAILED', error=$3, worker_id=NULL, lease_expires_at=NULL, updated_at=now() WHERE id=$1 AND worker_id=$2 AND status='RUNNING'")
            .bind(job_id).bind(worker_id).bind(error).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}
