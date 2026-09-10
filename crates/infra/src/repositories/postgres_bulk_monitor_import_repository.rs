use application::{
    dto::{
        BulkMonitorImportJob, BulkMonitorImportJobStatus, ClaimedBulkMonitorImportRow,
        CreateBulkMonitorImportJob, CreateBulkMonitorImportJobOutcome,
    },
    ports::{BulkMonitorImportRepoResult, BulkMonitorImportRepository},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresBulkMonitorImportRepository {
    pool: PgPool,
}

impl PostgresBulkMonitorImportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct JobRow {
    id: Uuid,
    tenant_id: Uuid,
    actor_user_id: Uuid,
    status: String,
    total_rows: i32,
    valid_rows: i32,
    invalid_rows: i32,
    processed_rows: i32,
    created_rows: i32,
    failed_rows: i32,
    skipped_rows: i32,
    forecast_days: i32,
    estimated_checks: i64,
    estimated_charge_units: i64,
    currency: String,
    batch_size: i32,
    pricing_snapshot: serde_json::Value,
    error_report: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

impl JobRow {
    fn into_domain(self) -> BulkMonitorImportRepoResult<BulkMonitorImportJob> {
        let status = match self.status.as_str() {
            "PENDING" => BulkMonitorImportJobStatus::Pending,
            "VALIDATING" => BulkMonitorImportJobStatus::Validating,
            "READY" => BulkMonitorImportJobStatus::Ready,
            "RUNNING" => BulkMonitorImportJobStatus::Running,
            "PAUSED" => BulkMonitorImportJobStatus::Paused,
            "COMPLETED" => BulkMonitorImportJobStatus::Completed,
            "FAILED" => BulkMonitorImportJobStatus::Failed,
            "CANCELLED" => BulkMonitorImportJobStatus::Cancelled,
            other => return Err(format!("unknown bulk import job status '{other}'").into()),
        };
        Ok(BulkMonitorImportJob {
            id: self.id,
            tenant_id: self.tenant_id,
            actor_user_id: self.actor_user_id,
            status,
            total_rows: self.total_rows,
            valid_rows: self.valid_rows,
            invalid_rows: self.invalid_rows,
            processed_rows: self.processed_rows,
            created_rows: self.created_rows,
            failed_rows: self.failed_rows,
            skipped_rows: self.skipped_rows,
            forecast_days: self.forecast_days,
            estimated_checks: self.estimated_checks,
            estimated_charge_units: self.estimated_charge_units,
            currency: self.currency,
            batch_size: self.batch_size,
            pricing_snapshot: self.pricing_snapshot,
            error_report: self.error_report,
            created_at: self.created_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
            updated_at: self.updated_at,
        })
    }
}

const JOB_COLUMNS: &str = "id, tenant_id, actor_user_id, status, total_rows, valid_rows, invalid_rows, \
processed_rows, created_rows, failed_rows, skipped_rows, forecast_days, estimated_checks, estimated_charge_units, currency, \
batch_size, pricing_snapshot, error_report, created_at, started_at, completed_at, updated_at";

#[async_trait]
impl BulkMonitorImportRepository for PostgresBulkMonitorImportRepository {
    async fn create_confirmed_job(
        &self,
        input: &CreateBulkMonitorImportJob,
    ) -> BulkMonitorImportRepoResult<CreateBulkMonitorImportJobOutcome> {
        let mut tx = self.pool.begin().await?;

        let inserted = sqlx::query_as::<_, JobRow>(&format!(
            "INSERT INTO monitor_bulk_import_jobs \
             (id, tenant_id, actor_user_id, status, total_rows, valid_rows, invalid_rows, \
              processed_rows, created_rows, failed_rows, skipped_rows, forecast_days, estimated_checks, estimated_charge_units, \
              currency, batch_size, idempotency_key, source_sha256, pricing_snapshot, confirmed_at) \
             VALUES ($1,$2,$3,'READY',$4,$5,$6,0,0,0,0,$7,$8,$9,$10,$11,$12,$13,$14,now()) \
             ON CONFLICT (tenant_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING \
             RETURNING {JOB_COLUMNS}"
        ))
        .bind(input.id)
        .bind(input.tenant_id)
        .bind(input.actor_user_id)
        .bind(input.total_rows)
        .bind(input.valid_rows)
        .bind(input.invalid_rows)
        .bind(input.forecast_days)
        .bind(input.estimated_checks)
        .bind(input.estimated_charge_units)
        .bind(&input.currency)
        .bind(input.batch_size)
        .bind(&input.idempotency_key)
        .bind(&input.source_sha256)
        .bind(&input.pricing_snapshot)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(job_row) = inserted else {
            let existing = sqlx::query_as::<_, JobRow>(&format!(
                "SELECT {JOB_COLUMNS} FROM monitor_bulk_import_jobs \
                 WHERE tenant_id = $1 AND idempotency_key = $2"
            ))
            .bind(input.tenant_id)
            .bind(&input.idempotency_key)
            .fetch_one(&mut *tx)
            .await?;

            let (existing_hash, existing_forecast): (Option<String>, i32) = sqlx::query_as(
                "SELECT source_sha256, forecast_days FROM monitor_bulk_import_jobs \
                 WHERE tenant_id = $1 AND idempotency_key = $2",
            )
            .bind(input.tenant_id)
            .bind(&input.idempotency_key)
            .fetch_one(&mut *tx)
            .await?;

            tx.rollback().await?;
            if existing_hash.as_deref() != Some(input.source_sha256.as_str())
                || existing_forecast != input.forecast_days
            {
                return Ok(CreateBulkMonitorImportJobOutcome::IdempotencyConflict);
            }
            return Ok(CreateBulkMonitorImportJobOutcome::Existing(
                existing.into_domain()?,
            ));
        };

        if !input.rows.is_empty() {
            let mut builder: QueryBuilder<'_, Postgres> = QueryBuilder::new(
                "INSERT INTO monitor_bulk_import_rows \
                 (job_id, row_number, payload, validation_status, validation_errors, planned_monitor_id) ",
            );
            builder.push_values(&input.rows, |mut b, row| {
                b.push_bind(input.id)
                    .push_bind(row.row_number)
                    .push_bind(row.payload.clone())
                    .push_bind(&row.validation_status)
                    .push_bind(row.validation_errors.clone())
                    .push_bind(row.planned_monitor_id);
            });
            builder.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(CreateBulkMonitorImportJobOutcome::Created(
            job_row.into_domain()?,
        ))
    }

    async fn find_job_for_tenant(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<Option<BulkMonitorImportJob>> {
        let row = sqlx::query_as::<_, JobRow>(&format!(
            "SELECT {JOB_COLUMNS} FROM monitor_bulk_import_jobs \
             WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(job_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(JobRow::into_domain).transpose()
    }

    async fn find_job_by_idempotency_key(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> BulkMonitorImportRepoResult<Option<(BulkMonitorImportJob, Option<String>, i32)>> {
        let row = sqlx::query_as::<_, JobRow>(&format!(
            "SELECT {JOB_COLUMNS} FROM monitor_bulk_import_jobs
             WHERE tenant_id=$1 AND idempotency_key=$2",
        ))
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let (source_sha256, forecast_days): (Option<String>, i32) = sqlx::query_as(
            "SELECT source_sha256, forecast_days
             FROM monitor_bulk_import_jobs
             WHERE tenant_id=$1 AND idempotency_key=$2",
        )
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_one(&self.pool)
        .await?;

        Ok(Some((row.into_domain()?, source_sha256, forecast_days)))
    }

    async fn claim_jobs(
        &self,
        worker_id: Uuid,
        limit: u32,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<Vec<BulkMonitorImportJob>> {
        let rows = sqlx::query_as::<_, JobRow>(&format!(
            "WITH candidates AS ( \
                SELECT id FROM monitor_bulk_import_jobs \
                WHERE status = 'READY' \
                   OR (status = 'RUNNING' AND lease_expires_at < now()) \
                ORDER BY created_at ASC \
                FOR UPDATE SKIP LOCKED \
                LIMIT $1 \
             ) \
             UPDATE monitor_bulk_import_jobs j \
             SET status='RUNNING', lease_owner=$2, lease_expires_at=$3, \
                 attempt_count=attempt_count+1, started_at=COALESCE(started_at, now()), \
                 updated_at=now() \
             FROM candidates c WHERE j.id=c.id \
             RETURNING {JOB_COLUMNS}"
        ))
        .bind(i64::from(limit))
        .bind(worker_id)
        .bind(lease_until)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(JobRow::into_domain).collect()
    }

    async fn renew_job_lease(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        lease_until: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<bool> {
        let result = sqlx::query(
            "UPDATE monitor_bulk_import_jobs \
             SET lease_expires_at=$3, updated_at=now() \
             WHERE id=$1 AND lease_owner=$2 AND status='RUNNING'",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(lease_until)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn claim_rows(
        &self,
        job_id: Uuid,
        limit: u32,
        stale_before: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<Vec<ClaimedBulkMonitorImportRow>> {
        #[derive(FromRow)]
        struct Row {
            id: i64,
            job_id: Uuid,
            row_number: i32,
            payload: serde_json::Value,
            planned_monitor_id: Uuid,
            attempt_count: i32,
        }

        let rows = sqlx::query_as::<_, Row>(
            "WITH candidates AS ( \
                SELECT r.id FROM monitor_bulk_import_rows r \
                 JOIN monitor_bulk_import_jobs j ON j.id = r.job_id \
                 WHERE r.job_id=$1 AND j.status='RUNNING' \
                  AND ( \
                    (validation_status='VALID' AND (next_attempt_at IS NULL OR next_attempt_at <= now())) \
                    OR (validation_status='PROCESSING' AND updated_at < $3) \
                  ) \
                ORDER BY row_number ASC \
                FOR UPDATE SKIP LOCKED \
                LIMIT $2 \
             ) \
             UPDATE monitor_bulk_import_rows r \
             SET validation_status='PROCESSING', attempt_count=attempt_count+1, \
                 next_attempt_at=NULL, updated_at=now() \
             FROM candidates c WHERE r.id=c.id \
             RETURNING r.id, r.job_id, r.row_number, r.payload, r.planned_monitor_id, r.attempt_count",
        )
        .bind(job_id)
        .bind(i64::from(limit))
        .bind(stale_before)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ClaimedBulkMonitorImportRow {
                id: row.id,
                job_id: row.job_id,
                row_number: row.row_number,
                payload: row.payload,
                planned_monitor_id: row.planned_monitor_id,
                attempt_count: row.attempt_count,
            })
            .collect())
    }

    async fn mark_row_created(
        &self,
        row_id: i64,
        monitor_id: Uuid,
    ) -> BulkMonitorImportRepoResult<()> {
        sqlx::query(
            "UPDATE monitor_bulk_import_rows \
             SET validation_status='CREATED', monitor_id=$2, last_error=NULL, \
                 next_attempt_at=NULL, updated_at=now() \
             WHERE id=$1",
        )
        .bind(row_id)
        .bind(monitor_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn retry_row(
        &self,
        row_id: i64,
        error: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> BulkMonitorImportRepoResult<()> {
        sqlx::query(
            "UPDATE monitor_bulk_import_rows \
             SET validation_status='VALID', last_error=$2, next_attempt_at=$3, updated_at=now() \
             WHERE id=$1",
        )
        .bind(row_id)
        .bind(error)
        .bind(next_attempt_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn fail_row(&self, row_id: i64, error: &str) -> BulkMonitorImportRepoResult<()> {
        sqlx::query(
            "UPDATE monitor_bulk_import_rows \
             SET validation_status='FAILED', last_error=$2, next_attempt_at=NULL, updated_at=now() \
             WHERE id=$1",
        )
        .bind(row_id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn skip_row(&self, row_id: i64, reason: &str) -> BulkMonitorImportRepoResult<()> {
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 1_000 {
            return Err("invalid bulk import skip reason".into());
        }
        sqlx::query(
            "UPDATE monitor_bulk_import_rows              SET validation_status='SKIPPED', last_error=$2, next_attempt_at=NULL, updated_at=now()              WHERE id=$1",
        )
        .bind(row_id)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn pause_job(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<bool> {
        let result = sqlx::query(
            "UPDATE monitor_bulk_import_jobs
             SET status='PAUSED', lease_owner=NULL, lease_expires_at=NULL, updated_at=now()
             WHERE id=$1 AND tenant_id=$2 AND status IN ('READY','RUNNING')",
        )
        .bind(job_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn resume_job(
        &self,
        job_id: Uuid,
        tenant_id: Uuid,
    ) -> BulkMonitorImportRepoResult<bool> {
        let result = sqlx::query(
            "UPDATE monitor_bulk_import_jobs
             SET status='READY', updated_at=now()
             WHERE id=$1 AND tenant_id=$2 AND status='PAUSED'",
        )
        .bind(job_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn refresh_job_state(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
    ) -> BulkMonitorImportRepoResult<BulkMonitorImportJob> {
        let row = sqlx::query_as::<_, JobRow>(&format!(
            "WITH counts AS ( \
                SELECT \
                  COUNT(*) FILTER (WHERE validation_status='CREATED')::int AS created_count, \
                  COUNT(*) FILTER (WHERE validation_status='FAILED')::int AS failed_count, \
                  COUNT(*) FILTER (WHERE validation_status='INVALID')::int AS invalid_count, \
                   COUNT(*) FILTER (WHERE validation_status='SKIPPED')::int AS skipped_count, \
                   COUNT(*) FILTER (WHERE validation_status IN ('VALID','PROCESSING'))::int AS remaining_count, \
                  COALESCE( \
                    jsonb_agg( \
                      jsonb_build_object( \
                        'row', row_number, \
                        'status', validation_status, \
                        'errors', COALESCE(validation_errors, to_jsonb(last_error)) \
                      ) ORDER BY row_number \
                    ) FILTER (WHERE validation_status IN ('INVALID','FAILED')), \
                    '[]'::jsonb \
                  ) AS errors \
                FROM monitor_bulk_import_rows WHERE job_id=$1 \
             ) \
             UPDATE monitor_bulk_import_jobs j \
             SET processed_rows=c.created_count+c.failed_count+c.invalid_count+c.skipped_count, \
                 created_rows=c.created_count, failed_rows=c.failed_count, invalid_rows=c.invalid_count, skipped_rows=c.skipped_count, \
                 error_report=CASE WHEN c.remaining_count=0 THEN c.errors ELSE error_report END, \
                 status=CASE WHEN c.remaining_count=0 THEN 'COMPLETED' ELSE 'RUNNING' END, \
                 completed_at=CASE WHEN c.remaining_count=0 THEN COALESCE(completed_at, now()) ELSE completed_at END, \
                 lease_owner=CASE WHEN c.remaining_count=0 THEN NULL ELSE lease_owner END, \
                 lease_expires_at=CASE WHEN c.remaining_count=0 THEN NULL ELSE lease_expires_at END, \
                 updated_at=now() \
             FROM counts c \
             WHERE j.id=$1 AND j.lease_owner=$2 \
             RETURNING {JOB_COLUMNS}"
        ))
        .bind(job_id)
        .bind(worker_id)
        .fetch_one(&self.pool)
        .await?;
        row.into_domain()
    }

    async fn release_job_after_error(
        &self,
        job_id: Uuid,
        worker_id: Uuid,
        error: &str,
    ) -> BulkMonitorImportRepoResult<()> {
        sqlx::query(
            "UPDATE monitor_bulk_import_jobs \
             SET status='READY', lease_owner=NULL, lease_expires_at=NULL, \
                 last_error=$3, updated_at=now() \
             WHERE id=$1 AND lease_owner=$2 AND status='RUNNING'",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
