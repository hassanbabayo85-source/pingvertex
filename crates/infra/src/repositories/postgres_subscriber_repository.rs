use application::ports::SubscriberRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Email, MonitorId, Subscriber, SubscriberId, TenantId};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed subscriber repository.
#[derive(Debug, Clone)]
pub struct PostgresSubscriberRepository {
    pool: PgPool,
}

impl PostgresSubscriberRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct SubscriberRow {
    id: Uuid,
    monitor_id: Uuid,
    tenant_id: Uuid,
    email: String,
    verified: bool,
    created_at: DateTime<Utc>,
}

impl SubscriberRow {
    fn from_domain(subscriber: &Subscriber) -> Self {
        Self {
            id: subscriber.id.into_uuid(),
            monitor_id: subscriber.monitor_id.into_uuid(),
            tenant_id: subscriber.tenant_id.0,
            email: subscriber.email.as_str().to_string(),
            verified: subscriber.verified,
            created_at: subscriber.created_at,
        }
    }

    fn into_domain(self) -> RepoResult<Subscriber> {
        Ok(Subscriber {
            id: SubscriberId::from_uuid(self.id),
            monitor_id: MonitorId::from_uuid(self.monitor_id),
            tenant_id: TenantId(self.tenant_id),
            email: Email::new(self.email)?,
            verified: self.verified,
            created_at: self.created_at,
        })
    }
}

#[async_trait]
impl SubscriberRepository for PostgresSubscriberRepository {
    async fn save(&self, subscriber: &Subscriber) -> RepoResult<()> {
        let row = SubscriberRow::from_domain(subscriber);
        sqlx::query(
            r#"
            INSERT INTO subscribers (id, monitor_id, tenant_id, email, verified, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO UPDATE
            SET verified = EXCLUDED.verified
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.tenant_id)
        .bind(row.email)
        .bind(row.verified)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: SubscriberId) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM subscribers
            WHERE id = $1
            "#,
        )
        .bind(id.into_uuid())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id(&self, id: SubscriberId) -> RepoResult<Option<Subscriber>> {
        let row = sqlx::query_as::<_, SubscriberRow>(
            r#"
            SELECT id, monitor_id, tenant_id, email, verified, created_at
            FROM subscribers
            WHERE id = $1
            "#,
        )
        .bind(id.into_uuid())
        .fetch_optional(&self.pool)
        .await?;

        row.map(SubscriberRow::into_domain).transpose()
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Subscriber>> {
        let rows = sqlx::query_as::<_, SubscriberRow>(
            r#"
            SELECT id, monitor_id, tenant_id, email, verified, created_at
            FROM subscribers
            WHERE monitor_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(monitor_id.into_uuid())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(SubscriberRow::into_domain).collect()
    }
}
