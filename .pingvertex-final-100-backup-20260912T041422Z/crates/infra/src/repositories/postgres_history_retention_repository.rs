use std::error::Error;

use application::ports::HistoryRetentionRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::TenantId;
use sqlx::PgPool;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Postgres implementation of batched tenant-scoped check-history
/// deletion.
#[derive(Debug, Clone)]
pub struct PostgresHistoryRetentionRepository {
    pool: PgPool,
}

impl PostgresHistoryRetentionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HistoryRetentionRepository for PostgresHistoryRetentionRepository {
    async fn delete_older_than_batch(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> RepoResult<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM checks
            WHERE id IN (
                SELECT c.id
                FROM checks c
                JOIN monitors m ON c.monitor_id = m.id
                WHERE m.tenant_id = $1
                  AND c.checked_at < $2
                LIMIT $3
            )
            "#,
        )
        .bind(tenant_id.0)
        .bind(cutoff)
        .bind(batch_size as i64)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
