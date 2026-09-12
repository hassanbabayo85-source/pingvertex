use std::error::Error;
use std::time::Duration;

use application::ports::BillingWebhookIdempotencyRepository;
use application::ports::billing_webhook_idempotency_repository::WebhookIdempotencyClaim;
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

    async fn try_claim(
        &self,
        provider_event_id: &str,
        stale_timeout: Duration,
    ) -> Result<WebhookIdempotencyClaim, Box<dyn Error + Send + Sync>> {
        let stale_seconds = stale_timeout.as_secs() as i64;

        let claimed_status: Option<String> = sqlx::query_scalar(
            r#"
            INSERT INTO billing_webhook_idempotency (
                provider_event_id,
                status,
                processing_started_at
            )
            VALUES ($1, 'processing', now())
            ON CONFLICT (provider_event_id) DO UPDATE
            SET status = 'processing',
                processing_started_at = now(),
                processed_at = NULL
            WHERE billing_webhook_idempotency.status = 'pending'
               OR (
                    billing_webhook_idempotency.status = 'processing'
                    AND billing_webhook_idempotency.processing_started_at <=
                        now() - make_interval(secs => $2)
               )
            RETURNING status
            "#,
        )
        .bind(provider_event_id)
        .bind(stale_seconds)
        .fetch_optional(&self.pool)
        .await?;

        if claimed_status.is_some() {
            return Ok(WebhookIdempotencyClaim::Claimed);
        }

        let current: String = sqlx::query_scalar(
            "SELECT status FROM billing_webhook_idempotency WHERE provider_event_id = $1",
        )
        .bind(provider_event_id)
        .fetch_one(&self.pool)
        .await?;

        match current.as_str() {
            "processed" => Ok(WebhookIdempotencyClaim::AlreadyProcessed),
            _ => Ok(WebhookIdempotencyClaim::InProgress),
        }
    }

    async fn mark_processed(
        &self,
        provider_event_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query(
            r#"
            UPDATE billing_webhook_idempotency
            SET status = 'processed',
                processing_started_at = NULL,
                processed_at = now()
            WHERE provider_event_id = $1
            "#,
        )
        .bind(provider_event_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_pending(
        &self,
        provider_event_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        sqlx::query(
            r#"
            UPDATE billing_webhook_idempotency
            SET status = 'pending',
                processing_started_at = NULL,
                processed_at = NULL
            WHERE provider_event_id = $1
            "#,
        )
        .bind(provider_event_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
