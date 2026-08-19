use std::error::Error;
use std::sync::Arc;

use application::ports::{
    EncryptedProxyCredentials, ProxyCheckJobRepository, ProxyCredentialCipher,
};
use async_trait::async_trait;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::enums::{ProxyCheckJobStatus, ProxyScheme};
use domain::shared::ids::{ProxyCheckJobId, ProxyEntryId, TenantId, UserId};
use domain::value_objects::ProxyHost;
use domain::value_objects::ProxyResult;
use sqlx::{FromRow, PgPool};
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

//
// proxy_check_jobs table row
//
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

//
// proxy_entries table row
//
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
    /// Converts this row into a domain `ProxyEntry`, decrypting
    /// `encrypted_credentials` (if present) via `cipher`.
    ///
    /// SECURITY: the decrypted `ProxyCredentials` only ever lives in the
    /// returned `ProxyEntry` in memory — it is never logged, and
    /// `ProxyCredentials`'s own hand-written `Debug` impl redacts the
    /// password even if something downstream does log the entry.
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

/// PostgreSQL-backed repository for `ProxyCheckJob` and its `ProxyEntry`
/// children.
///
/// # Why this repository owns a cipher
///
/// `ProxyEntry::credentials()` holds *plaintext* `ProxyCredentials`, but
/// the `proxy_entries.encrypted_credentials` column (migration 0020) is
/// `BYTEA` — ciphertext only, by design (see the SECURITY note on that
/// column). Something has to perform that encryption before the INSERT
/// happens (and decryption on the way back out), and
/// `ProxyCheckJobRepository`'s methods (as defined in
/// `application::ports`) do not thread a cipher through as a parameter —
/// nor does any composition-root wiring for this repository exist yet to
/// suggest an alternative. Given Clean Architecture (infra implements a
/// contract application/domain own, and *how* encryption happens is
/// explicitly an infra decision per `ProxyCredentialCipher`'s own doc
/// comment), the most faithful choice available at this layer is for
/// this repository to hold the cipher itself and encrypt/decrypt
/// immediately at the persistence boundary. If a future prompt wires
/// this differently (e.g. encryption pushed up into an application-layer
/// use case instead), this field can be removed then.
pub struct PostgresProxyCheckJobRepository {
    pool: PgPool,
    cipher: Arc<dyn ProxyCredentialCipher>,
}

impl PostgresProxyCheckJobRepository {
    pub fn new(pool: PgPool, cipher: Arc<dyn ProxyCredentialCipher>) -> Self {
        Self { pool, cipher }
    }
}

impl std::fmt::Debug for PostgresProxyCheckJobRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Manual impl (rather than `#[derive(Debug)]`): `dyn
        // ProxyCredentialCipher` does not implement `Debug` (it isn't a
        // supertrait), and even if it did, this repository should never
        // print anything about the cipher/key it holds.
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
    /// Persists `job` together with all of `entries` in a single database
    /// transaction: if any insert fails partway through, the whole
    /// transaction is rolled back (nothing is committed) so a job can
    /// never end up visible in storage without its entries, and no
    /// partial/inconsistent entry set is ever persisted.
    async fn save_job(&self, job: &ProxyCheckJob, entries: &[ProxyEntry]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;

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
        .execute(&mut *tx)
        .await?;

        for entry in entries {
            // SECURITY: credentials are encrypted here, immediately
            // before the INSERT, and never bound/logged in plaintext.
            // `ProxyCredentialCipherError` implements `std::error::Error`,
            // so `?` converts it into this function's `RepoResult` error
            // type without any credential material passing through a
            // format string.
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
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    /// Persists an update to the job's own status/progress fields only —
    /// entries are updated independently via `update_entry`. A
    /// single-statement UPDATE needs no explicit transaction: it is
    /// already atomic.
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

    /// Looks up a single job by id. Does NOT load its entries — callers
    /// that need both should call `find_entries_by_job_id` separately
    /// (mirrors `ProxyCheckJobRepository`'s doc comment: job and entry
    /// lifecycles are managed independently after creation, since the
    /// worker checks/updates entries concurrently and separately from
    /// the job's own status).
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

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(row.into_domain()?))
    }

    /// Lists all jobs owned by `user_id`, most recent first. Ordered by
    /// `created_at DESC` to match the `idx_proxy_check_jobs_user_id_created_at`
    /// index (migration 0019), which was created specifically to make
    /// this per-user query (and the per-user daily quota check it
    /// supports) fast.
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

    /// Tenant-scoped equivalent of `find_jobs_by_user_id`. Returns every
    /// job belonging to `tenant_id`, regardless of which member of the
    /// tenant submitted it.
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

    /// Persists the result of a single checked entry. Only `result_json`
    /// is written — `host`/`port`/`scheme`/`encrypted_credentials` never
    /// change after `save_job` first inserts an entry, so there is
    /// nothing else for the worker to update here.
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

    /// Lists all entries belonging to `job_id`, decrypting each entry's
    /// credentials (if any) via `self.cipher`. Uses the
    /// `idx_proxy_entries_job_id` index (migration 0020).
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

    /// Finds up to `limit` `Pending` jobs, oldest first. Used by the
    /// worker's poll loop to discover new proxy-check work.
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
