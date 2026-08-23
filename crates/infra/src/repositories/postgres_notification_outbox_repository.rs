use std::error::Error;

use application::ports::NotificationOutboxRepository;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use domain::{
    NotificationOutboxEntry, NotificationOutboxEntryId, NotificationOutboxEventKind,
    NotificationOutboxStatus,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresNotificationOutboxRepository {
    pool: PgPool,
}

impl PostgresNotificationOutboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct NotificationOutboxRow {
    id: Uuid,
    channel_id: Uuid,
    event_kind: String,
    payload: String,
    idempotency_key: String,
    status: String,
    attempts: i32,
    retry_at: DateTime<Utc>,
    processing_started_at: Option<DateTime<Utc>>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NotificationOutboxRow {
    fn into_domain(self) -> RepoResult<NotificationOutboxEntry> {
        let status = match self.status.as_str() {
            "pending" => NotificationOutboxStatus::Pending,
            "processing" => NotificationOutboxStatus::Processing,
            "sent" => NotificationOutboxStatus::Sent,
            "failed" => NotificationOutboxStatus::Failed,
            other => {
                return Err(
                    format!("unknown notification_outbox.status '{other}' in database").into(),
                );
            }
        };

        let event_kind = match self.event_kind.as_str() {
            "monitor_down" => NotificationOutboxEventKind::MonitorDown,
            "monitor_recovered" => NotificationOutboxEventKind::MonitorRecovered,
            "test" => NotificationOutboxEventKind::Test,
            other => {
                return Err(format!(
                    "unknown notification_outbox.event_kind '{other}' in database"
                )
                .into());
            }
        };

        Ok(NotificationOutboxEntry {
            id: NotificationOutboxEntryId(self.id),
            channel_id: domain::shared::ids::NotificationChannelId(self.channel_id),
            event_kind,
            payload: self.payload,
            idempotency_key: self.idempotency_key,
            status,
            attempts: self.attempts,
            retry_at: self.retry_at,
            processing_started_at: self.processing_started_at,
            error: self.error,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl NotificationOutboxRepository for PostgresNotificationOutboxRepository {
    async fn enqueue(&self, entry: &NotificationOutboxEntry) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO notification_outbox (
                id,
                channel_id,
                event_kind,
                payload,
                idempotency_key,
                status,
                attempts,
                retry_at,
                processing_started_at,
                error,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(entry.id.0)
        .bind(entry.channel_id.0)
        .bind(entry.event_kind.as_str())
        .bind(&entry.payload)
        .bind(&entry.idempotency_key)
        .bind(entry.status.as_str())
        .bind(entry.attempts)
        .bind(entry.retry_at)
        .bind(entry.processing_started_at)
        .bind(&entry.error)
        .bind(entry.created_at)
        .bind(entry.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn recover_stale_processing(&self, timeout: Duration) -> RepoResult<u64> {
        let cutoff = Utc::now() - timeout;

        let result = sqlx::query(
            r#"
            UPDATE notification_outbox
            SET status = 'pending',
                processing_started_at = NULL,
                updated_at = now()
            WHERE status = 'processing'
              AND processing_started_at IS NOT NULL
              AND processing_started_at <= $1
            "#,
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn claim_next(&self) -> RepoResult<Option<NotificationOutboxEntry>> {
        let row = sqlx::query_as::<_, NotificationOutboxRow>(
            r#"
            UPDATE notification_outbox
            SET status = 'processing',
                processing_started_at = now(),
                updated_at = now()
            WHERE id = (
                SELECT id
                FROM notification_outbox
                WHERE status = 'pending'
                  AND retry_at <= now()
                ORDER BY created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING
                id,
                channel_id,
                event_kind,
                payload,
                idempotency_key,
                status,
                attempts,
                retry_at,
                processing_started_at,
                error,
                created_at,
                updated_at
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(NotificationOutboxRow::into_domain).transpose()
    }

    async fn mark_sent(&self, id: NotificationOutboxEntryId) -> RepoResult<()> {
        sqlx::query(
            r#"
            UPDATE notification_outbox
            SET status = 'sent',
                processing_started_at = NULL,
                error = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_failed(
        &self,
        id: NotificationOutboxEntryId,
        error: String,
        retry_at: DateTime<Utc>,
        max_attempts: i32,
    ) -> RepoResult<NotificationOutboxStatus> {
        let status: String = sqlx::query_scalar(
            r#"
            UPDATE notification_outbox
            SET attempts = attempts + 1,
                status = CASE
                    WHEN attempts + 1 >= $2 THEN 'failed'
                    ELSE 'pending'
                END,
                retry_at = CASE
                    WHEN attempts + 1 >= $2 THEN retry_at
                    ELSE $3
                END,
                error = $1,
                processing_started_at = NULL,
                updated_at = now()
            WHERE id = $4
            RETURNING status
            "#,
        )
        .bind(error)
        .bind(max_attempts)
        .bind(retry_at)
        .bind(id.0)
        .fetch_one(&self.pool)
        .await?;

        match status.as_str() {
            "pending" => Ok(NotificationOutboxStatus::Pending),
            "failed" => Ok(NotificationOutboxStatus::Failed),
            other => Err(format!(
                "unexpected notification_outbox.status '{other}' after mark_failed"
            )
            .into()),
        }
    }
}
