use application::ports::{PaymentRepoResult, PaymentRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::Payment;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
pub struct PostgresPaymentRepository {
    pool: PgPool,
}
impl PostgresPaymentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(FromRow)]
struct Row {
    id: Uuid,
    tenant_id: Uuid,
    amount_units: i64,
    currency: String,
    status: String,
    provider: String,
    provider_payment_id: Option<String>,
    idempotency_key: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl Row {
    fn into_domain(self) -> Payment {
        Payment {
            id: self.id,
            tenant_id: self.tenant_id,
            amount_units: self.amount_units,
            currency: self.currency,
            status: self.status,
            provider: self.provider,
            provider_payment_id: self.provider_payment_id,
            idempotency_key: self.idempotency_key,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
#[async_trait]
impl PaymentRepository for PostgresPaymentRepository {
    async fn record(&self, p: &Payment) -> PaymentRepoResult<()> {
        sqlx::query("INSERT INTO payments(id,tenant_id,amount_units,currency,status,provider,provider_payment_id,idempotency_key,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (tenant_id,idempotency_key) DO UPDATE SET
                status=CASE
                    WHEN payments.status IN ('succeeded','paid','completed')
                         AND EXCLUDED.status IN ('failed','pending')
                    THEN payments.status
                    ELSE EXCLUDED.status
                END,
                provider_payment_id=COALESCE(payments.provider_payment_id,EXCLUDED.provider_payment_id),
                updated_at=EXCLUDED.updated_at").bind(p.id).bind(p.tenant_id).bind(p.amount_units).bind(&p.currency).bind(&p.status).bind(&p.provider).bind(&p.provider_payment_id).bind(&p.idempotency_key).bind(p.created_at).bind(p.updated_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn find_by_provider_payment_id(
        &self,
        provider: &str,
        pid: &str,
    ) -> PaymentRepoResult<Option<Payment>> {
        Ok(sqlx::query_as::<_,Row>("SELECT id,tenant_id,amount_units,currency,status,provider,provider_payment_id,idempotency_key,created_at,updated_at FROM payments WHERE provider=$1 AND provider_payment_id=$2").bind(provider).bind(pid).fetch_optional(&self.pool).await?.map(Row::into_domain))
    }
}
