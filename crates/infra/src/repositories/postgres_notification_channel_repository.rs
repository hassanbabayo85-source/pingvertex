use std::error::Error;

use application::ports::NotificationChannelRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::NotificationChannel,
    enums::NotificationChannelKind,
    shared::ids::{MonitorId, NotificationChannelId, UserId},
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed notification channel repository.
///
/// Every method enforces ownership at the SQL level (`WHERE ... AND
/// user_id = $N`) rather than relying on callers to check first — see the
/// `NotificationChannelRepository` trait doc comment for why.
#[derive(Debug, Clone)]
pub struct PostgresNotificationChannelRepository {
    pool: PgPool,
}

impl PostgresNotificationChannelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct NotificationChannelRow {
    id: Uuid,
    monitor_id: Uuid,
    user_id: Uuid,
    kind: String,
    target: String,
    signing_secret: Option<String>,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NotificationChannelRow {
    fn from_domain(channel: &NotificationChannel) -> Self {
        Self {
            id: channel.id.0,
            monitor_id: channel.monitor_id.0,
            user_id: channel.user_id.0,
            kind: channel.kind.to_string(),
            target: channel.target.clone(),
            signing_secret: channel.signing_secret.clone(),
            enabled: channel.enabled,
            created_at: channel.created_at,
            updated_at: channel.updated_at,
        }
    }

    fn into_domain(self) -> RepoResult<NotificationChannel> {
        let kind = match self.kind.as_str() {
            "email" => NotificationChannelKind::Email,
            "webhook" => NotificationChannelKind::Webhook,
            other => {
                return Err(
                    format!("unknown notification_channels.kind '{other}' in database").into(),
                );
            }
        };

        Ok(NotificationChannel {
            id: NotificationChannelId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            user_id: UserId(self.user_id),
            kind,
            target: self.target,
            signing_secret: self.signing_secret,
            enabled: self.enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const SELECT_COLUMNS: &str =
    "id, monitor_id, user_id, kind, target, signing_secret, enabled, created_at, updated_at";

#[async_trait]
impl NotificationChannelRepository for PostgresNotificationChannelRepository {
    async fn create(&self, channel: &NotificationChannel) -> RepoResult<()> {
        let row = NotificationChannelRow::from_domain(channel);

        sqlx::query(
            r#"
            INSERT INTO notification_channels (
                id, monitor_id, user_id, kind, target, signing_secret, enabled, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.user_id)
        .bind(row.kind)
        .bind(row.target)
        .bind(row.signing_secret)
        .bind(row.enabled)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<bool> {
        let result =
            sqlx::query("DELETE FROM notification_channels WHERE id = $1 AND user_id = $2")
                .bind(id.0)
                .bind(user_id.0)
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id_for_user(
        &self,
        id: NotificationChannelId,
        user_id: UserId,
    ) -> RepoResult<Option<NotificationChannel>> {
        let row = sqlx::query_as::<_, NotificationChannelRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM notification_channels WHERE id = $1 AND user_id = $2"
        ))
        .bind(id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(NotificationChannelRow::into_domain).transpose()
    }

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<NotificationChannel>>> {
        // First confirm the monitor itself belongs to `user_id` — a
        // monitor that exists but isn't theirs must return `None` (→ the
        // handler returns 404), not `Some(vec![])` (→ 200 with an empty
        // list, which would leak "this monitor id exists").
        let owns_monitor: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM monitors WHERE id = $1 AND user_id = $2)",
        )
        .bind(monitor_id.0)
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await?;

        if !owns_monitor {
            return Ok(None);
        }

        let rows = sqlx::query_as::<_, NotificationChannelRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM notification_channels \
             WHERE monitor_id = $1 AND user_id = $2 ORDER BY created_at ASC"
        ))
        .bind(monitor_id.0)
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;

        let channels = rows
            .into_iter()
            .map(NotificationChannelRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Some(channels))
    }

    async fn list_enabled_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> RepoResult<Vec<NotificationChannel>> {
        let rows = sqlx::query_as::<_, NotificationChannelRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM notification_channels \
             WHERE monitor_id = $1 AND enabled = TRUE ORDER BY created_at ASC"
        ))
        .bind(monitor_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(NotificationChannelRow::into_domain)
            .collect()
    }
}
