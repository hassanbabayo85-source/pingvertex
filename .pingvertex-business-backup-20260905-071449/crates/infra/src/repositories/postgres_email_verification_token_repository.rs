use std::error::Error;

use application::ports::EmailVerificationTokenRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{EmailVerificationToken, EmailVerificationTokenId, UserId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresEmailVerificationTokenRepository {
    pool: PgPool,
}

impl PostgresEmailVerificationTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct EmailVerificationTokenRow {
    id: Uuid,
    user_id: Uuid,
    token_prefix: String,
    token_hash: String,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl EmailVerificationTokenRow {
    fn from_domain(token: &EmailVerificationToken) -> Self {
        Self {
            id: token.id.into_uuid(),
            user_id: token.user_id.into_uuid(),
            token_prefix: token.token_prefix.clone(),
            token_hash: token.token_hash.clone(),
            expires_at: token.expires_at,
            used_at: token.used_at,
            created_at: token.created_at,
        }
    }

    fn into_domain(self) -> EmailVerificationToken {
        EmailVerificationToken {
            id: EmailVerificationTokenId::from_uuid(self.id),
            user_id: UserId::from_uuid(self.user_id),
            token_prefix: self.token_prefix,
            token_hash: self.token_hash,
            expires_at: self.expires_at,
            used_at: self.used_at,
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl EmailVerificationTokenRepository for PostgresEmailVerificationTokenRepository {
    async fn create(&self, token: &EmailVerificationToken) -> RepoResult<()> {
        let row = EmailVerificationTokenRow::from_domain(token);
        sqlx::query(
            r#"
            INSERT INTO email_verification_tokens
                (id, user_id, token_prefix, token_hash, expires_at, used_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.token_prefix)
        .bind(row.token_hash)
        .bind(row.expires_at)
        .bind(row.used_at)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_prefix(
        &self,
        token_prefix: &str,
    ) -> RepoResult<Option<EmailVerificationToken>> {
        let row = sqlx::query_as::<_, EmailVerificationTokenRow>(
            r#"
            SELECT id, user_id, token_prefix, token_hash, expires_at, used_at, created_at
            FROM email_verification_tokens
            WHERE token_prefix = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(token_prefix)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(EmailVerificationTokenRow::into_domain))
    }

    async fn mark_used(
        &self,
        id: EmailVerificationTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        sqlx::query(
            r#"
            UPDATE email_verification_tokens
            SET used_at = $2
            WHERE id = $1 AND used_at IS NULL
            "#,
        )
        .bind(id.into_uuid())
        .bind(used_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_unused_for_user(&self, user_id: UserId) -> RepoResult<()> {
        sqlx::query(
            r#"
            DELETE FROM email_verification_tokens
            WHERE user_id = $1 AND used_at IS NULL
            "#,
        )
        .bind(user_id.into_uuid())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn try_consume(
        &self,
        id: EmailVerificationTokenId,
        used_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        // SEC: single atomic statement, same pattern as
        // PostgresResetPasswordTokenRepository::try_consume.
        let result = sqlx::query(
            r#"
            UPDATE email_verification_tokens
            SET used_at = $2
            WHERE id = $1 AND used_at IS NULL
            "#,
        )
        .bind(id.into_uuid())
        .bind(used_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
