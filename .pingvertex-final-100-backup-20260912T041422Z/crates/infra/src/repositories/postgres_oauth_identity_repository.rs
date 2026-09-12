use application::ports::{OAuthIdentityRepoResult, OAuthIdentityRepository};
use async_trait::async_trait;
use domain::UserId;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresOAuthIdentityRepository {
    pool: PgPool,
}

impl PostgresOAuthIdentityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn valid_provider(provider: &str) -> bool {
    matches!(provider, "google" | "apple")
}

#[async_trait]
impl OAuthIdentityRepository for PostgresOAuthIdentityRepository {
    async fn find_user_id(
        &self,
        provider: &str,
        subject: &str,
    ) -> OAuthIdentityRepoResult<Option<UserId>> {
        if !valid_provider(provider) || subject.trim().is_empty() {
            return Err("invalid OAuth identity".into());
        }
        let id: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM oauth_identities WHERE provider = $1 AND subject = $2",
        )
        .bind(provider)
        .bind(subject)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.map(UserId::from_uuid))
    }

    async fn link(
        &self,
        provider: &str,
        subject: &str,
        user_id: UserId,
    ) -> OAuthIdentityRepoResult<()> {
        if !valid_provider(provider) || subject.trim().is_empty() {
            return Err("invalid OAuth identity".into());
        }
        sqlx::query("INSERT INTO oauth_identities(provider, subject, user_id) VALUES ($1,$2,$3)")
            .bind(provider)
            .bind(subject)
            .bind(user_id.as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
