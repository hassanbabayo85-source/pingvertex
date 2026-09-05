use std::error::Error;
use std::sync::Arc;

use application::ports::monitor_repository::QuotaCheckOutcome;
use application::ports::{
    EncryptedProxyCredentials, ProxyCheckJobRepository, ProxyCredentialCipher,
};
use async_trait::async_trait;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::enums::{ProxyCheckJobStatus, ProxyScheme};
use domain::shared::ids::{ProxyCheckJobId, ProxyEntryId, TenantId, UserId};
use domain::value_objects::ProxyHost;
use domain::value_objects::ProxyResult;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn job_status_to_db(status: &ProxyCheckJobStatus) -> &'static str {
    match status {
        ProxyCheckJobStatus::Pending => "pending",
        ProxyCheckJobStatus::Running => "running",
        ProxyCheckJobStatus::Completed => "completed",
        ProxyCheckJobStatus::Failed => "failed",
    }
}

fn job_status_from_db(value: &str) -> RepoResult<ProxyCheckJobStatus> {
    match value {
        "pending" => Ok(ProxyCheckJobStatus::Pending),
        "running" => Ok(ProxyCheckJobStatus::Running),
        "completed" => Ok(ProxyCheckJobStatus::Completed),
        "failed" => Ok(ProxyCheckJobStatus::Failed),
        other => Err(format!("unsupported proxy check job status in database: {other}").into()),
    }
}

fn proxy_scheme_to_db(scheme: &ProxyScheme) -> &'static str {
    match scheme {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks5 => "socks5",
    }
}

fn proxy_scheme_from_db(value: &str) -> RepoResult<ProxyScheme> {
    match value {
        "http" => Ok(ProxyScheme::Http),
        "https" => Ok(ProxyScheme::Https),
        "socks5" => Ok(ProxyScheme::Socks5),
        other => Err(format!("unsupported proxy scheme in database: {other}").into()),
    }
}

#[derive(Debug, Clone, FromRow)]
struct ProxyCheckJobRow {
    id: Uuid,
    user_id: Uuid,
    tenant_id: Uuid,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    total_count: i32,
    checked_count: i32,
}

impl ProxyCheckJobRow {
    fn into_domain(self) -> RepoResult<ProxyCheckJob> {
        let status = job_status_from_db(&self.status)?;
        let total_count: u32 = self.total_count.try_into()?;
        let checked_count: u32 = self.checked_count.try_into()?;

        Ok(ProxyCheckJob::from_persisted(
            ProxyCheckJobId(self.id),
            UserId(self.user_id),
            TenantId(self.tenant_id),
            status,
            self.created_at,
            self.completed_at,
            total_count,
            checked_count,
        ))
    }
}

#[derive(Debug, Clone, FromRow)]
struct ProxyEntryRow {
    id: Uuid,
    job_id: Uuid,
    host: String,
    port: i32,
    scheme: String,
    encrypted_credentials: Option<Vec<u8>>,
    result_json: Option<serde_json::Value>,
}

impl ProxyEntryRow {
    fn into_domain(self, cipher: &Arc<dyn ProxyCredentialCipher>) -> RepoResult<ProxyEntry> {
        let host = ProxyHost::new(self.host)?;
        let port: u16 = self.port.try_into()?;
        let scheme = proxy_scheme_from_db(&self.scheme)?;

        let credentials = match self.encrypted_credentials {
            Some(bytes) => Some(cipher.decrypt(&EncryptedProxyCredentials::from(bytes))?),
            None => None,
        };

        let result: Option<ProxyResult> =
            self.result_json.map(serde_json::from_value).transpose()?;

        Ok(ProxyEntry::from_persisted(
            ProxyEntryId(self.id),
            ProxyCheckJobId(self.job_id),
            host,
            port,
            scheme,
            credentials,
            result,
        ))
    }
}

pub struct PostgresProxyCheckJobRepository {
    pool: PgPool,
    cipher: Arc<dyn ProxyCredentialCipher>,
}

impl PostgresProxyCheckJobRepository {
    pub fn new(pool: PgPool, cipher: Arc<dyn ProxyCredentialCipher>) -> Self {
        Self { pool, cipher }
    }

    async fn insert_job_and_entries(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job: &ProxyCheckJob,
        entries: &[ProxyEntry],
    ) -> RepoResult<()> {
        let total_count: i32 = job.total_count().try_into()?;
        let checked_count: i32 = job.checked_count().try_into()?;

        sqlx::query(
            r#"
            INSERT INTO proxy_check_jobs (
                id,
                user_id,
                tenant_id,
                status,
                created_at,
                completed_at,
                total_count,
                checked_count
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(job.id().0)
        .bind(job.user_id().0)
        .bind(job.tenant_id().0)
        .bind(job_status_to_db(&job.status()))
        .bind(job.created_at())
        .bind(job.completed_at())
        .bind(total_count)
        .bind(checked_count)
        .execute(&mut **tx)
        .await?;

        for entry in entries {
            let encrypted_credentials: Option<Vec<u8>> = match entry.credentials() {
                Some(credentials) => Some(self.cipher.encrypt(credentials)?.as_bytes().to_vec()),
                None => None,
            };

            let result_json = entry.result().map(serde_json::to_value).transpose()?;
            let port: i32 = i32::from(entry.port());

            sqlx::query(
                r#"
                INSERT INTO proxy_entries (
                    id,
                    job_id,
                    host,
                    port,
                    scheme,
                    encrypted_credentials,
                    result_json
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(entry.id().0)
            .bind(entry.job_id().0)
            .bind(entry.host().as_str())
            .bind(port)
            .bind(proxy_scheme_to_db(entry.scheme()))
            .bind(encrypted_credentials)
            .bind(result_json)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }
}

impl std::fmt::Debug for PostgresProxyCheckJobRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresProxyCheckJobRepository").finish()
    }
}

impl Clone for PostgresProxyCheckJobRepository {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            cipher: Arc::clone(&self.cipher),
        }
    }
}

#[async_trait]
impl ProxyCheckJobRepository for PostgresProxyCheckJobRepository {
    async fn save_job(&self, job: &ProxyCheckJob, entries: &[ProxyEntry]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;
        self.insert_job_and_entries(&mut tx, job, entries).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn save_job_if_within_quota(
        &self,
        job: &ProxyCheckJob,
        entries: &[ProxyEntry],
        user_id: UserId,
        limit: usize,
    ) -> RepoResult<QuotaCheckOutcome> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(format!("proxy_check_jobs:{}", user_id.0))
            .execute(&mut *tx)
            .await?;

        let current_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM proxy_check_jobs
            WHERE user_id = $1
              AND created_at >= date_trunc('day', now())
            "#,
        )
        .bind(user_id.0)
        .fetch_one(&mut *tx)
        .await?;

        if current_count as usize >= limit {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }

        self.insert_job_and_entries(&mut tx, job, entries).await?;
        tx.commit().await?;

        Ok(QuotaCheckOutcome::Saved)
    }

    async fn try_mark_running(&self, id: ProxyCheckJobId) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            UPDATE proxy_check_jobs
            SET status = 'running',
                completed_at = NULL
            WHERE id = $1
              AND status = 'pending'
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_job(&self, job: &ProxyCheckJob) -> RepoResult<()> {
        let checked_count: i32 = job.checked_count().try_into()?;

        sqlx::query(
            r#"
            UPDATE proxy_check_jobs
            SET status = $1,
                completed_at = $2,
                checked_count = $3
            WHERE id = $4
            "#,
        )
        .bind(job_status_to_db(&job.status()))
        .bind(job.completed_at())
        .bind(checked_count)
        .bind(job.id().0)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_job_by_id(&self, id: ProxyCheckJobId) -> RepoResult<Option<ProxyCheckJob>> {
        let row = sqlx::query_as::<_, ProxyCheckJobRow>(
            r#"
            SELECT id, user_id, tenant_id, status, created_at, completed_at, total_count, checked_count
            FROM proxy_check_jobs
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(ProxyCheckJobRow::into_domain).transpose()
    }

    async fn find_jobs_by_user_id(&self, user_id: UserId) -> RepoResult<Vec<ProxyCheckJob>> {
        let rows = sqlx::query_as::<_, ProxyCheckJobRow>(
            r#"
            SELECT id, user_id, tenant_id, status, created_at, completed_at, total_count, checked_count
            FROM proxy_check_jobs
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(ProxyCheckJobRow::into_domain)
            .collect()
    }

    async fn find_jobs_by_tenant_id(&self, tenant_id: TenantId) -> RepoResult<Vec<ProxyCheckJob>> {
        let rows = sqlx::query_as::<_, ProxyCheckJobRow>(
            r#"
            SELECT id, user_id, tenant_id, status, created_at, completed_at, total_count, checked_count
            FROM proxy_check_jobs
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(ProxyCheckJobRow::into_domain)
            .collect()
    }

    async fn update_entry(&self, entry: &ProxyEntry) -> RepoResult<()> {
        let result_json = entry.result().map(serde_json::to_value).transpose()?;

        sqlx::query(
            r#"
            UPDATE proxy_entries
            SET result_json = $1
            WHERE id = $2
            "#,
        )
        .bind(result_json)
        .bind(entry.id().0)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_entries_by_job_id(&self, job_id: ProxyCheckJobId) -> RepoResult<Vec<ProxyEntry>> {
        let rows = sqlx::query_as::<_, ProxyEntryRow>(
            r#"
            SELECT id, job_id, host, port, scheme, encrypted_credentials, result_json
            FROM proxy_entries
            WHERE job_id = $1
            "#,
        )
        .bind(job_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| row.into_domain(&self.cipher))
            .collect()
    }

    async fn find_pending_jobs(&self, limit: usize) -> RepoResult<Vec<ProxyCheckJob>> {
        let limit_i32: i32 = limit.try_into()?;

        let rows = sqlx::query_as::<_, ProxyCheckJobRow>(
            r#"
            SELECT id, user_id, tenant_id, status, created_at, completed_at, total_count, checked_count
            FROM proxy_check_jobs
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit_i32)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(ProxyCheckJobRow::into_domain)
            .collect()
    }
}
