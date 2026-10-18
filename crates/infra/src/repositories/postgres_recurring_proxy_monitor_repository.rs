//! PostgreSQL-backed `RecurringProxyMonitorRepository`.
//!
//! # Credential handling
//!
//! Proxy credentials are encrypted with `ProxyCredentialCipher` before
//! being written to `encrypted_credentials BYTEA`, and decrypted on
//! every read. They are never logged, never returned in any response,
//! and never stored in plaintext — the cipher is the single boundary
//! responsible for the transformation.
//!
//! If decryption fails (rotated key, corrupted row), the row is loaded
//! with `credentials = None` and a `tracing::warn!` is emitted. This
//! preserves operational availability (a monitor keeps working
//! anonymously) at the cost of a visible log line for the operator.
//!
//! # Concurrency
//!
//! * `claim_due_monitors` uses `FOR UPDATE SKIP LOCKED` inside a CTE
//!   that is part of the same `UPDATE ... RETURNING` statement, so a
//!   monitor can be claimed by exactly one worker per lease window.
//! * `save_monitor_if_within_quota` uses `pg_advisory_xact_lock` to
//!   serialize the count-then-insert against concurrent inserts for
//!   the same tenant, matching `PostgresMonitorRepository::save_if_within_quota`.

use std::error::Error;
use std::sync::Arc;

use application::ports::monitor_repository::QuotaCheckOutcome;
use application::ports::{
    ClaimedRecurringProxyMonitor, EncryptedProxyCredentials, ProxyCredentialCipher,
    RecurringProxyMonitorRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{RecurringProxyCheck, RecurringProxyCheckStatus, RecurringProxyMonitor};
use domain::enums::ProxyScheme;
use domain::shared::ids::{TenantId, UserId};
use domain::value_objects::{ProxyCredentials, ProxyHost, ProxyResult};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn boxed<E: Error + Send + Sync + 'static>(e: E) -> Box<dyn Error + Send + Sync> {
    Box::new(e)
}

fn scheme_to_db(s: &ProxyScheme) -> &'static str {
    match s {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks4 => "socks4",
        ProxyScheme::Socks5 => "socks5",
    }
}

fn scheme_from_db(s: &str) -> RepoResult<ProxyScheme> {
    match s {
        "http" => Ok(ProxyScheme::Http),
        "https" => Ok(ProxyScheme::Https),
        "socks4" => Ok(ProxyScheme::Socks4),
        "socks5" => Ok(ProxyScheme::Socks5),
        other => Err(format!("unknown proxy scheme in database: {other}").into()),
    }
}

const MONITOR_COLUMNS: &str = "id, tenant_id, user_id, name, host, port, scheme, \
    encrypted_credentials, interval_seconds, user_enabled, entitlement_suspended, \
    next_run_at, last_run_at, worker_id, lease_expires_at, created_at, updated_at";

const CHECK_COLUMNS: &str = "id, monitor_id, tenant_id, status, started_at, finished_at, \
    result_json, error_message";

#[derive(Debug, FromRow)]
struct MonitorRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    name: String,
    host: String,
    port: i32,
    scheme: String,
    encrypted_credentials: Option<Vec<u8>>,
    interval_seconds: i32,
    user_enabled: bool,
    entitlement_suspended: bool,
    next_run_at: Option<DateTime<Utc>>,
    last_run_at: Option<DateTime<Utc>>,
    // Read from RETURNING in `claim_due_monitors`; the claimed monitor
    // takes these values from the query parameters, so the fields
    // themselves are never accessed from Rust. Kept in the row struct
    // so sqlx can deserialize RETURNING output without a schema drift
    // error.
    #[allow(dead_code)]
    worker_id: Option<String>,
    #[allow(dead_code)]
    lease_expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct CheckRow {
    id: Uuid,
    monitor_id: Uuid,
    tenant_id: Uuid,
    status: String,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    result_json: Option<serde_json::Value>,
    error_message: Option<String>,
}

// ── Repository ──────────────────────────────────────────────────────

pub struct PostgresRecurringProxyMonitorRepository {
    pool: PgPool,
    cipher: Arc<dyn ProxyCredentialCipher>,
}

impl PostgresRecurringProxyMonitorRepository {
    pub fn new(pool: PgPool, cipher: Arc<dyn ProxyCredentialCipher>) -> Self {
        Self { pool, cipher }
    }

    fn decrypt_credentials(&self, encrypted: Option<Vec<u8>>) -> Option<ProxyCredentials> {
        let bytes = encrypted?;
        match self
            .cipher
            .decrypt(&EncryptedProxyCredentials::from(bytes))
        {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to decrypt recurring proxy credentials; treating this monitor as anonymous"
                );
                None
            }
        }
    }

    fn encrypt_credentials(&self, creds: &ProxyCredentials) -> RepoResult<Vec<u8>> {
        Ok(self
            .cipher
            .encrypt(creds)
            .map_err(boxed)?
            .as_bytes()
            .to_vec())
    }

    fn row_to_monitor(&self, row: MonitorRow) -> RepoResult<RecurringProxyMonitor> {
        let credentials = self.decrypt_credentials(row.encrypted_credentials);
        let scheme = scheme_from_db(&row.scheme)?;
        let host = ProxyHost::new(row.host).map_err(boxed)?;

        Ok(RecurringProxyMonitor {
            id: row.id,
            tenant_id: TenantId(row.tenant_id),
            user_id: UserId(row.user_id),
            name: row.name,
            host,
            port: row.port as u16,
            scheme,
            credentials,
            interval_seconds: row.interval_seconds as u32,
            user_enabled: row.user_enabled,
            entitlement_suspended: row.entitlement_suspended,
            next_run_at: row.next_run_at,
            last_run_at: row.last_run_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    fn row_to_check(row: CheckRow) -> RepoResult<RecurringProxyCheck> {
        let status = row
            .status
            .parse::<RecurringProxyCheckStatus>()
            .map_err(boxed)?;
        let result: Option<ProxyResult> = row
            .result_json
            .and_then(|v| serde_json::from_value(v).ok());
        Ok(RecurringProxyCheck {
            id: row.id,
            monitor_id: row.monitor_id,
            tenant_id: TenantId(row.tenant_id),
            status,
            started_at: row.started_at,
            finished_at: row.finished_at,
            result,
            error_message: row.error_message,
        })
    }

    async fn insert_monitor(
        tx: &mut Transaction<'_, Postgres>,
        monitor: &RecurringProxyMonitor,
        encrypted_credentials: Option<Vec<u8>>,
    ) -> RepoResult<()> {
        sqlx::query(
            r#"
            INSERT INTO recurring_proxy_monitors (
                id, tenant_id, user_id, name, host, port, scheme,
                encrypted_credentials, interval_seconds, user_enabled,
                entitlement_suspended, next_run_at, last_run_at,
                created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            "#,
        )
        .bind(monitor.id)
        .bind(monitor.tenant_id.0)
        .bind(monitor.user_id.0)
        .bind(&monitor.name)
        .bind(monitor.host.as_str())
        .bind(i32::from(monitor.port))
        .bind(scheme_to_db(&monitor.scheme))
        .bind(encrypted_credentials)
        .bind(monitor.interval_seconds as i32)
        .bind(monitor.user_enabled)
        .bind(monitor.entitlement_suspended)
        .bind(monitor.next_run_at)
        .bind(monitor.last_run_at)
        .bind(monitor.created_at)
        .bind(monitor.updated_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl RecurringProxyMonitorRepository for PostgresRecurringProxyMonitorRepository {
    async fn save_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()> {
        let encrypted = monitor
            .credentials
            .as_ref()
            .map(|c| self.encrypt_credentials(c))
            .transpose()?;

        let mut tx = self.pool.begin().await?;

        // Update-on-conflict path: same shape as `insert_monitor`, but
        // tenant-scoped so a leaked id can't cross-update.
        sqlx::query(
            r#"
            INSERT INTO recurring_proxy_monitors (
                id, tenant_id, user_id, name, host, port, scheme,
                encrypted_credentials, interval_seconds, user_enabled,
                entitlement_suspended, next_run_at, last_run_at,
                created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                host = EXCLUDED.host,
                port = EXCLUDED.port,
                scheme = EXCLUDED.scheme,
                encrypted_credentials = EXCLUDED.encrypted_credentials,
                interval_seconds = EXCLUDED.interval_seconds,
                user_enabled = EXCLUDED.user_enabled,
                entitlement_suspended = EXCLUDED.entitlement_suspended,
                next_run_at = EXCLUDED.next_run_at,
                last_run_at = EXCLUDED.last_run_at,
                updated_at = EXCLUDED.updated_at
            WHERE recurring_proxy_monitors.tenant_id = EXCLUDED.tenant_id
            "#,
        )
        .bind(monitor.id)
        .bind(monitor.tenant_id.0)
        .bind(monitor.user_id.0)
        .bind(&monitor.name)
        .bind(monitor.host.as_str())
        .bind(i32::from(monitor.port))
        .bind(scheme_to_db(&monitor.scheme))
        .bind(encrypted)
        .bind(monitor.interval_seconds as i32)
        .bind(monitor.user_enabled)
        .bind(monitor.entitlement_suspended)
        .bind(monitor.next_run_at)
        .bind(monitor.last_run_at)
        .bind(monitor.created_at)
        .bind(monitor.updated_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn save_monitor_if_within_quota(
        &self,
        monitor: &RecurringProxyMonitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let encrypted = monitor
            .credentials
            .as_ref()
            .map(|c| self.encrypt_credentials(c))
            .transpose()?;

        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(format!("recurring_proxy_monitors:{}", tenant_id.0))
            .execute(&mut *tx)
            .await?;

        let current: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM recurring_proxy_monitors WHERE tenant_id = $1")
                .bind(tenant_id.0)
                .fetch_one(&mut *tx)
                .await?;

        if current as u32 >= limit {
            // Drop the tx — implicit rollback releases the advisory lock.
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }

        Self::insert_monitor(&mut tx, monitor, encrypted).await?;
        tx.commit().await?;
        Ok(QuotaCheckOutcome::Saved)
    }

    async fn find_by_id(
        &self,
        id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyMonitor>> {
        let q = format!(
            "SELECT {MONITOR_COLUMNS} FROM recurring_proxy_monitors WHERE id = $1 AND tenant_id = $2"
        );
        let row = sqlx::query_as::<_, MonitorRow>(&q)
            .bind(id)
            .bind(tenant_id.0)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => Ok(Some(self.row_to_monitor(r)?)),
            None => Ok(None),
        }
    }

    async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<RecurringProxyMonitor>> {
        let q = format!(
            "SELECT {MONITOR_COLUMNS} FROM recurring_proxy_monitors WHERE tenant_id = $1 ORDER BY created_at DESC"
        );
        let rows = sqlx::query_as::<_, MonitorRow>(&q)
            .bind(tenant_id.0)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(|r| self.row_to_monitor(r)).collect()
    }

    async fn update_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()> {
        let encrypted = monitor
            .credentials
            .as_ref()
            .map(|c| self.encrypt_credentials(c))
            .transpose()?;

        let result = sqlx::query(
            r#"
            UPDATE recurring_proxy_monitors SET
                name = $2,
                host = $3,
                port = $4,
                scheme = $5,
                encrypted_credentials = $6,
                interval_seconds = $7,
                user_enabled = $8,
                entitlement_suspended = $9,
                next_run_at = $10,
                last_run_at = $11,
                updated_at = $12
            WHERE id = $1 AND tenant_id = $13
            "#,
        )
        .bind(monitor.id)
        .bind(&monitor.name)
        .bind(monitor.host.as_str())
        .bind(i32::from(monitor.port))
        .bind(scheme_to_db(&monitor.scheme))
        .bind(encrypted)
        .bind(monitor.interval_seconds as i32)
        .bind(monitor.user_enabled)
        .bind(monitor.entitlement_suspended)
        .bind(monitor.next_run_at)
        .bind(monitor.last_run_at)
        .bind(monitor.updated_at)
        .bind(monitor.tenant_id.0)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err("recurring proxy monitor not found for this tenant".into());
        }
        Ok(())
    }

    async fn delete_for_tenant(&self, id: Uuid, tenant_id: TenantId) -> RepoResult<bool> {
        // `recurring_proxy_checks` cascades on delete via ON DELETE CASCADE
        // (migration 0125), so one statement is enough.
        let result = sqlx::query(
            "DELETE FROM recurring_proxy_monitors WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM recurring_proxy_monitors WHERE tenant_id = $1")
                .bind(tenant_id.0)
                .fetch_one(&self.pool)
                .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn claim_due_monitors(
        &self,
        now: DateTime<Utc>,
        worker_id: &str,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> RepoResult<Vec<ClaimedRecurringProxyMonitor>> {
        // The whole claim happens in one statement:
        //   1. `FOR UPDATE SKIP LOCKED` inside the CTE picks a batch of
        //      due rows without blocking on any concurrent worker.
        //   2. The outer UPDATE writes the lease for exactly those rows
        //      and returns their post-update shape.
        //
        // Because the rows are already locked and updated inside the
        // same statement, a second worker running the same query
        // concurrently sees them as already leased and skips them.
        let q = format!(
            r#"
            WITH due AS (
                SELECT id
                FROM recurring_proxy_monitors
                WHERE user_enabled = TRUE
                  AND entitlement_suspended = FALSE
                  AND next_run_at IS NOT NULL
                  AND next_run_at <= $1
                  AND (lease_expires_at IS NULL OR lease_expires_at < $1)
                ORDER BY next_run_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE recurring_proxy_monitors m
            SET worker_id = $3, lease_expires_at = $4
            FROM due
            WHERE m.id = due.id
            RETURNING {MONITOR_COLUMNS}
            "#
        );

        let rows = sqlx::query_as::<_, MonitorRow>(&q)
            .bind(now)
            .bind(i64::from(limit))
            .bind(worker_id)
            .bind(lease_until)
            .fetch_all(&self.pool)
            .await?;

        let mut claimed = Vec::with_capacity(rows.len());
        for row in rows {
            let monitor = self.row_to_monitor(row)?;
            claimed.push(ClaimedRecurringProxyMonitor {
                monitor,
                worker_id: worker_id.to_owned(),
                lease_expires_at: lease_until,
                claimed_at: now,
            });
        }
        // Preserve the `ORDER BY next_run_at ASC` even though RETURNING
        // does not guarantee it.
        claimed.sort_by_key(|c| c.monitor.next_run_at);
        Ok(claimed)
    }

    async fn reschedule(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
        last_run_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            UPDATE recurring_proxy_monitors
            SET worker_id = NULL,
                lease_expires_at = NULL,
                last_run_at = $4,
                next_run_at = $5,
                updated_at = now()
            WHERE id = $1
              AND tenant_id = $2
              AND worker_id = $3
            "#,
        )
        .bind(id)
        .bind(tenant_id.0)
        .bind(worker_id)
        .bind(last_run_at)
        .bind(next_run_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn release_lease(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
    ) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            UPDATE recurring_proxy_monitors
            SET worker_id = NULL,
                lease_expires_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND tenant_id = $2
              AND worker_id = $3
            "#,
        )
        .bind(id)
        .bind(tenant_id.0)
        .bind(worker_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn save_check(&self, check: &RecurringProxyCheck) -> RepoResult<()> {
        let result_json: Option<serde_json::Value> = check
            .result
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok());

        sqlx::query(
            r#"
            INSERT INTO recurring_proxy_checks (
                id, monitor_id, tenant_id, status, started_at, finished_at,
                result_json, error_message
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(check.id)
        .bind(check.monitor_id)
        .bind(check.tenant_id.0)
        .bind(check.status.as_str())
        .bind(check.started_at)
        .bind(check.finished_at)
        .bind(result_json)
        .bind(&check.error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn finish_check(&self, check: &RecurringProxyCheck) -> RepoResult<bool> {
        let result_json: Option<serde_json::Value> = check
            .result
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok());

        let result = sqlx::query(
            r#"
            UPDATE recurring_proxy_checks
            SET status = $2,
                finished_at = $3,
                result_json = $4,
                error_message = $5
            WHERE id = $1
            "#,
        )
        .bind(check.id)
        .bind(check.status.as_str())
        .bind(check.finished_at)
        .bind(result_json)
        .bind(&check.error_message)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_checks(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
        offset: u32,
        limit: u32,
    ) -> RepoResult<(Vec<RecurringProxyCheck>, u64)> {
        let q = format!(
            "SELECT {CHECK_COLUMNS} FROM recurring_proxy_checks WHERE monitor_id = $1 AND tenant_id = $2 ORDER BY started_at DESC LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query_as::<_, CheckRow>(&q)
            .bind(monitor_id)
            .bind(tenant_id.0)
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM recurring_proxy_checks WHERE monitor_id = $1 AND tenant_id = $2",
        )
        .bind(monitor_id)
        .bind(tenant_id.0)
        .fetch_one(&self.pool)
        .await?;

        let checks = rows
            .into_iter()
            .map(Self::row_to_check)
            .collect::<RepoResult<Vec<_>>>()?;
        Ok((checks, u64::try_from(total).unwrap_or(0)))
    }

    async fn find_latest_check(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyCheck>> {
        let q = format!(
            "SELECT {CHECK_COLUMNS} FROM recurring_proxy_checks WHERE monitor_id = $1 AND tenant_id = $2 ORDER BY started_at DESC LIMIT 1"
        );
        let row = sqlx::query_as::<_, CheckRow>(&q)
            .bind(monitor_id)
            .bind(tenant_id.0)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(r) => Ok(Some(Self::row_to_check(r)?)),
            None => Ok(None),
        }
    }
}
