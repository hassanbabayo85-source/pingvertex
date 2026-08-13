use std::error::Error;

use application::ports::CheckRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{CheckId, MonitorId, UserId};
use domain::{Check, CheckStatus, ErrorType};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
    status: i16,
    http_status: Option<i32>,
    response_time_ms: i64,
    error_type: String,
    error_message: Option<String>,
    checked_at: DateTime<Utc>,
    cert_expires_at: Option<DateTime<Utc>>,
    cert_days_remaining: Option<i64>,
}

impl CheckRow {
    fn from_domain(check: &Check) -> Self {
        Self {
            id: check.id.0,
            monitor_id: check.monitor_id.0,
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

        Ok(Check {
            id: CheckId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            status,
            status_code: self.http_status.map(|v| v as u16),
            latency_ms: self.response_time_ms as u128,
            error_type,
            error_message: self.error_message,
            checked_at: self.checked_at,
            cert_expires_at: None,
            cert_days_remaining: None,
        }
        .with_cert_info(self.cert_expires_at, self.cert_days_remaining))
    }
}

#[async_trait]
impl CheckRepository for PostgresCheckRepository {
    async fn save(&self, check: &Check) -> RepoResult<()> {
        let row = CheckRow::from_domain(check);

        sqlx::query(
            r#"
            INSERT INTO checks (
                id,
                monitor_id,
                status,
                http_status,
                response_time_ms,
                error_type,
                error_message,
                checked_at,
                cert_expires_at,
                cert_days_remaining
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.status)
        .bind(row.http_status)
        .bind(row.response_time_ms)
        .bind(row.error_type)
        .bind(row.error_message)
        .bind(row.checked_at)
        .bind(row.cert_expires_at)
        .bind(row.cert_days_remaining)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: CheckId) -> RepoResult<Option<Check>> {
        let row = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
                id,
                monitor_id,
                status,
                http_status,
                response_time_ms,
                error_type,
                error_message,
                checked_at,
                cert_expires_at,
                cert_days_remaining
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

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Check>> {
        let rows = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
                id,
                monitor_id,
                status,
                http_status,
                response_time_ms,
                error_type,
                error_message,
                checked_at,
                cert_expires_at,
                cert_days_remaining
            FROM checks
            WHERE monitor_id = $1
            ORDER BY checked_at DESC
            "#,
        )
        .bind(monitor_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(CheckRow::into_domain).collect()
    }

    async fn list_checks(
        &self,
        limit: u32,
        offset: u32,
        user_id: UserId,
    ) -> RepoResult<Vec<Check>> {
        let rows = sqlx::query_as::<_, CheckRow>(
            r#"
            SELECT
                c.id,
                c.monitor_id,
                c.status,
                c.http_status,
                c.response_time_ms,
                c.error_type,
                c.error_message,
                c.checked_at,
                c.cert_expires_at,
                c.cert_days_remaining
            FROM checks c
            JOIN monitors m ON c.monitor_id = m.id
            WHERE m.user_id = $3
            ORDER BY c.checked_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .bind(user_id.0) // UUID
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
