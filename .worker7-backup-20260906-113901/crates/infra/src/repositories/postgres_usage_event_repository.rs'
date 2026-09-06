use application::ports::{UsageEventRepoResult, UsageEventRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::UsageEvent;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresUsageEventRepository {
    pool: PgPool,
}
impl PostgresUsageEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct Row {
    id: Uuid,
    tenant_id: Uuid,
    operation: String,
    units_requested: i64,
    units_billed: i64,
    unit_price: i64,
    charge_units: i64,
    currency: String,
    pricing_version: i64,
    request_id: String,
    status: String,
    created_at: DateTime<Utc>,
}
impl Row {
    fn into_domain(self) -> UsageEvent {
        UsageEvent {
            id: self.id,
            tenant_id: self.tenant_id,
            operation: self.operation,
            units_requested: self.units_requested,
            units_billed: self.units_billed,
            unit_price: self.unit_price,
            charge_units: self.charge_units,
            currency: self.currency,
            pricing_version: self.pricing_version,
            request_id: self.request_id,
            status: self.status,
            created_at: self.created_at,
        }
    }
}
#[async_trait]
impl UsageEventRepository for PostgresUsageEventRepository {
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
                    pricing_version,request_id,status,created_at
             FROM api_usage_events WHERE tenant_id=$1 AND request_id=$2",
        ).bind(tenant_id).bind(request_id).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        let row = sqlx::query_as::<_, Row>(
            "INSERT INTO api_usage_events
             (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
              pricing_version,request_id,status)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT(tenant_id,request_id) DO UPDATE
             SET request_id=EXCLUDED.request_id
             RETURNING id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
                       pricing_version,request_id,status,created_at",
        ).bind(event.id).bind(event.tenant_id).bind(&event.operation)
         .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
         .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
         .bind(&event.request_id).bind(&event.status).fetch_one(&self.pool).await?;
        Ok(row.into_domain())
    }
}
