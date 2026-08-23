use std::error::Error;

use application::ports::BillingWebhookIdempotencyRepository;
use async_trait::async_trait;
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct PostgresBillingWebhookIdempotencyRepository {
    pool: PgPool,
}

impl PostgresBillingWebhookIdempotencyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BillingWebhookIdempotencyRepository for PostgresBillingWebhookIdempotencyRepository {
    async fn try_insert(
        &self,
        provider_event_id: &str,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let result = sqlx::query(
            "INSERT INTO billing_webhook_idempotency (provider_event_id) \
             VALUES ($1) ON CONFLICT (provider_event_id) DO NOTHING",
        )
        .bind(provider_event_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}
