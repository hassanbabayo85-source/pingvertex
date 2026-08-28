use std::error::Error;

use application::ports::CheckRepository;
use application::ports::check_repository::CheckFilters;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{CheckId, MonitorId, TenantId};
use domain::{Check, CheckStatus, ErrorType, RegionCode};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// B34: hard ceiling on `list_by_monitor`'s unfiltered result set. This
/// endpoint has no caller-supplied limit/offset (unlike
/// `list_by_monitor_paginated`), and is reachable from the unauthenticated
/// public status endpoints (`get_public_status`, `public_status_page`),
/// so it must never be allowed to scan/return a monitor's entire check
/// history. 1000 most-recent checks is far more than any of its current
/// callers (latest-check + uptime-percentage calculation, multi-region
/// incident evaluation) need.
const LIST_BY_MONITOR_LIMIT: i64 = 1000;

#[derive(Debug, Clone)]
pub struct PostgresCheckRepository {
    pool: PgPool,
}

impl PostgresCheckRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct CheckRow {
    id: Uuid,
    monitor_id: Uuid,
    region: String,
    status: i16,
    http_status: Option<i32>,
    response_time_ms: i64,
    error_type: String,
    error_message: Option<String>,
    checked_at: DateTime<Utc>,
    cert_expires_at: Option<DateTime<Utc>>,
    cert_days_remaining: Option<i64>,
    country_code: Option<String>,
}

impl CheckRow {
    fn from_domain(check: &Check) -> Self {
        Self {
            id: check.id.0,
            monitor_id: check.monitor_id.0,
            region: check.region.to_string(),
            status: if matches!(check.status, CheckStatus::Up) {
                1
            } else {
                0
            },
            http_status: check.status_code.map(i32::from),
            response_time_ms: check.latency_ms as i64,
            error_type: format!("{:?}", check.error_type),
            error_message: check.error_message.clone(),
            checked_at: check.checked_at,
            cert_expires_at: check.cert_expires_at,
            cert_days_remaining: check.cert_days_remaining,
            country_code: check.country_code.clone(),
        }
    }

    fn into_domain(self) -> RepoResult<Check> {
        let status = if self.status == 1 {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        };

        let error_type = match self.error_type.as_str() {
            "None" => ErrorType::None,
            "Dns" => ErrorType::Dns,
            "Timeout" => ErrorType::Timeout,
            "Connection" => ErrorType::Connection,
            "Ssl" => ErrorType::Ssl,
            "Http4xx" => ErrorType::Http4xx,
            "Http5xx" => ErrorType::Http5xx,
            _ => ErrorType::Unknown,
        };

        let region = RegionCode::new(self.region)?;

        Ok(Check {
            id: CheckId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            region,
            status,
            status_code: self.http_status.map(|v| v as u16),
            latency_ms: self.response_time_ms as u128,
            error_type,
            error_message: self.error_message,
            checked_at: self.checked_at,
            cert_expires_at: None,
            cert_days_remaining: None,
            country_code: None,
        }
        .with_cert_info(self.cert_expires_at, self.cert_days_remaining)
        .with_country_code(self.country_code))
    }
}

#[async_trait]
impl CheckRepository for PostgresCheckRepository {
    async fn save(&self, check: &Check) -> RepoResult<()> {
        self.save_batch(std::slice::from_ref(check)).await
    }

    async fn save_batch(&self, checks: &[Check]) -> RepoResult<()> {
        if checks.is_empty() {
            return Ok(());
        }

        // Multi-row INSERT for efficient batch writes.
        // Number of checks per batch is bounded by caller; we use the
        // standard sqlx bind-per-placeholder approach.
        let mut query = String::from(
            "INSERT INTO checks (id,monitor_id,region,status,http_status,response_time_ms,error_type,error_message,checked_at,cert_expires_at,cert_days_remaining,country_code) VALUES ",
        );
        let mut params: Vec<String> = Vec::with_capacity(checks.len());
        for i in 0..checks.len() {
            let base = i * 12;
            params.push(format!(
                "(${},${},${},${},${},${},${},${},${},${},${},${})",
                base + 1,
                base + 2,
                base + 3,
                base + 4,
                base + 5,
                base + 6,
                base + 7,
                base + 8,
                base + 9,
                base + 10,
                base + 11,
                base + 12
            ));
        }
        query.push_str(&params.join(","));

        let mut q = sqlx::query(&query);
        for check in checks {
            let row = CheckRow::from_domain(check);
            q = q
                .bind(row.id)
                .bind(row.monitor_id)
                .bind(row.region)
                .bind(row.status)
                .bind(row.http_status)
                .bind(row.response_time_ms)
                .bind(row.error_type)
                .bind(row.error_message)
                .bind(row.checked_at)
                .bind(row.cert_expires_at)
                .bind(row.cert_days_remaining)
                .bind(row.country_code);
        }

        q.execute(&self.pool).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: CheckId) -> RepoResult<Option<Check>> {
        let row = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
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
            FROM checks
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(row.into_domain()?)),
            None => Ok(None),
        }
    }

    /// B34: now bounded by `LIST_BY_MONITOR_LIMIT` (most-recent-first) —
    /// see the constant's doc comment. Previously unbounded, which meant
    /// a monitor with a long check history caused every call site
    /// (including the unauthenticated public status endpoints) to scan
    /// and materialize its ENTIRE history on every request.
    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>> {
        let rows = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
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
            FROM checks
            WHERE monitor_id = $1
            ORDER BY checked_at DESC
            LIMIT $2
            "#,
        )
        .bind(monitor_id.0)
        .bind(LIST_BY_MONITOR_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(CheckRow::into_domain).collect()
    }

    async fn list_checks(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<Check>> {
        let rows = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
                c.id,
                c.monitor_id,
                c.region,
                c.status,
                c.http_status,
                c.response_time_ms,
                c.error_type,
                c.error_message,
                c.checked_at,
                c.cert_expires_at,
                c.cert_days_remaining,
                c.country_code
            FROM checks c
            JOIN monitors m ON c.monitor_id = m.id
            WHERE m.tenant_id = $1
            ORDER BY c.checked_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id.0)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(CheckRow::into_domain).collect()
    }

    async fn list_checks_filtered(
        &self,
        limit: u32,
        offset: u32,
        tenant_id: TenantId,
        filters: CheckFilters,
    ) -> RepoResult<Vec<Check>> {
        let status: Option<i16> = filters.status.map(|s| {
            if matches!(s, CheckStatus::Up) {
                1i16
            } else {
                0i16
            }
        });
        let monitor_id: Option<Uuid> = filters.monitor_id.map(|m| m.0);
        let from = filters.from;
        let to = filters.to;

        let rows = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
                c.id,
                c.monitor_id,
                c.region,
                c.status,
                c.http_status,
                c.response_time_ms,
                c.error_type,
                c.error_message,
                c.checked_at,
                c.cert_expires_at,
                c.cert_days_remaining,
                c.country_code
            FROM checks c
            JOIN monitors m ON c.monitor_id = m.id
            WHERE m.tenant_id = $1
              AND ($2::uuid IS NULL OR c.monitor_id = $2)
              AND ($3::smallint IS NULL OR c.status = $3)
              AND ($4::timestamptz IS NULL OR c.checked_at >= $4)
              AND ($5::timestamptz IS NULL OR c.checked_at <= $5)
            ORDER BY c.checked_at DESC
            LIMIT $6 OFFSET $7
            "#,
        )
        .bind(tenant_id.0)
        .bind(monitor_id)
        .bind(status)
        .bind(from)
        .bind(to)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(CheckRow::into_domain).collect()
    }

    async fn list_by_monitor_paginated(
        &self,
        monitor_id: MonitorId,
        limit: u32,
        offset: u32,
        filters: CheckFilters,
    ) -> RepoResult<Vec<Check>> {
        let status: Option<i16> = filters.status.map(|s| {
            if matches!(s, CheckStatus::Up) {
                1i16
            } else {
                0i16
            }
        });
        let from = filters.from;
        let to = filters.to;

        let rows = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
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
            FROM checks
            WHERE monitor_id = $1
              AND ($2::smallint IS NULL OR status = $2)
              AND ($3::timestamptz IS NULL OR checked_at >= $3)
              AND ($4::timestamptz IS NULL OR checked_at <= $4)
            ORDER BY checked_at DESC
            LIMIT $5 OFFSET $6
            "#,
        )
        .bind(monitor_id.0)
        .bind(status)
        .bind(from)
        .bind(to)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(CheckRow::into_domain).collect()
    }

    async fn delete_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<u64> {
        let result = sqlx::query("DELETE FROM checks WHERE monitor_id = $1")
            .bind(monitor_id.0)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}
