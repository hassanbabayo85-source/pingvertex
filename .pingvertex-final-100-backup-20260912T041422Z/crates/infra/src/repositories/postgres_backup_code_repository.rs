use std::error::Error;

use application::ports::BackupCodeRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{BackupCode, BackupCodeId, UserId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed backup-code repository.
#[derive(Debug, Clone)]
pub struct PostgresBackupCodeRepository {
    pool: PgPool,
}

impl PostgresBackupCodeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct BackupCodeRow {
    id: Uuid,
    user_id: Uuid,
    code_hash: String,
    used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl BackupCodeRow {
    fn from_domain(code: &BackupCode) -> Self {
        Self {
            id: code.id.into_uuid(),
            user_id: code.user_id.into_uuid(),
            code_hash: code.code_hash.clone(),
            used_at: code.used_at,
            created_at: code.created_at,
        }
    }

    fn into_domain(self) -> BackupCode {
        BackupCode {
            id: BackupCodeId::from_uuid(self.id),
            user_id: UserId::from_uuid(self.user_id),
            code_hash: self.code_hash,
            used_at: self.used_at,
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl BackupCodeRepository for PostgresBackupCodeRepository {
    async fn create_many(&self, codes: &[BackupCode]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;

        for code in codes {
            let row = BackupCodeRow::from_domain(code);
            sqlx::query(
                r#"
                INSERT INTO backup_codes (id, user_id, code_hash, used_at, created_at)
                VALUES ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(row.id)
            .bind(row.user_id)
            .bind(row.code_hash)
            .bind(row.used_at)
            .bind(row.created_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn find_all_by_user(&self, user_id: UserId) -> RepoResult<Vec<BackupCode>> {
        let rows = sqlx::query_as::<_, BackupCodeRow>(
            r#"
            SELECT id, user_id, code_hash, used_at, created_at
            FROM backup_codes
            WHERE user_id = $1
            "#,
        )
        .bind(user_id.into_uuid())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(BackupCodeRow::into_domain).collect())
    }

    async fn mark_used(&self, id: BackupCodeId, used_at: DateTime<Utc>) -> RepoResult<bool> {
        let redeemed_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE backup_codes
            SET used_at = $2
            WHERE id = $1 AND used_at IS NULL
            RETURNING id
            "#,
        )
        .bind(id.into_uuid())
        .bind(used_at)
        .fetch_optional(&self.pool)
        .await?;

        Ok(redeemed_id.is_some())
    }

    async fn delete_all_for_user(&self, user_id: UserId) -> RepoResult<()> {
        sqlx::query(
            r#"
            DELETE FROM backup_codes
            WHERE user_id = $1
            "#,
        )
        .bind(user_id.into_uuid())
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
