use application::ports::{
    UsageDailyPoint, UsageEventRepoResult, UsageEventRepository, UsageOperationPoint, UsageReport,
};
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
    monitor_id: Option<Uuid>,
    execution_id: Option<Uuid>,
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
            monitor_id: self.monitor_id,
            execution_id: self.execution_id,
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
                    pricing_version,request_id,monitor_id,execution_id,status,created_at
             FROM api_usage_events WHERE tenant_id=$1 AND request_id=$2",
        ).bind(tenant_id).bind(request_id).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        let row = sqlx::query_as::<_, Row>(
            "INSERT INTO api_usage_events
             (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
              pricing_version,request_id,monitor_id,execution_id,status)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT(tenant_id,request_id) DO UPDATE
             SET request_id=EXCLUDED.request_id
             RETURNING id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
                       pricing_version,request_id,monitor_id,execution_id,status,created_at",
        ).bind(event.id).bind(event.tenant_id).bind(&event.operation)
         .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
         .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
         .bind(&event.request_id).bind(event.monitor_id).bind(event.execution_id).bind(&event.status).fetch_one(&self.pool).await?;
        Ok(row.into_domain())
    }
    async fn report(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        operation: Option<&str>,
        status: Option<&str>,
        limit: u32,
    ) -> UsageEventRepoResult<UsageReport> {
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,monitor_id,execution_id,status,created_at
             FROM api_usage_events WHERE tenant_id=$1 AND created_at >= $2 AND created_at < $3
             AND ($4::text IS NULL OR operation=$4) AND ($5::text IS NULL OR status=$5)
             ORDER BY created_at DESC LIMIT $6"
        ).bind(tenant_id).bind(from).bind(to).bind(operation).bind(status).bind(limit.clamp(1,500) as i64).fetch_all(&self.pool).await?;
        let today_usage: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(units_billed),0) FROM api_usage_events WHERE tenant_id=$1 AND created_at >= date_trunc('day', now()) AND created_at < date_trunc('day', now()) + interval '1 day' AND status='CAPTURED'").bind(tenant_id).fetch_one(&self.pool).await?;
        let month_usage: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(units_billed),0) FROM api_usage_events WHERE tenant_id=$1 AND created_at >= date_trunc('month', now()) AND created_at < date_trunc('month', now()) + interval '1 month' AND status='CAPTURED'").bind(tenant_id).fetch_one(&self.pool).await?;
        let month_spend: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(charge_units),0) FROM api_usage_events WHERE tenant_id=$1 AND created_at >= date_trunc('month', now()) AND created_at < date_trunc('month', now()) + interval '1 month' AND status='CAPTURED'").bind(tenant_id).fetch_one(&self.pool).await?;
        #[derive(sqlx::FromRow)]
        struct D {
            day: String,
            usage_units: i64,
            spend_units: i64,
        }
        let daily = sqlx::query_as::<_, D>("SELECT to_char(created_at::date,'YYYY-MM-DD') day, COALESCE(SUM(units_billed),0) usage_units, COALESCE(SUM(charge_units),0) spend_units FROM api_usage_events WHERE tenant_id=$1 AND created_at >= $2 AND created_at < $3 AND status='CAPTURED' GROUP BY created_at::date ORDER BY created_at::date").bind(tenant_id).bind(from).bind(to).fetch_all(&self.pool).await?
            .into_iter().map(|r| UsageDailyPoint { day:r.day, usage_units:r.usage_units, spend_units:r.spend_units }).collect();
        #[derive(sqlx::FromRow)]
        struct O {
            operation: String,
            usage_units: i64,
            spend_units: i64,
        }
        let by_operation = sqlx::query_as::<_, O>("SELECT operation, COALESCE(SUM(units_billed),0) usage_units, COALESCE(SUM(charge_units),0) spend_units FROM api_usage_events WHERE tenant_id=$1 AND created_at >= $2 AND created_at < $3 AND status='CAPTURED' GROUP BY operation ORDER BY spend_units DESC").bind(tenant_id).bind(from).bind(to).fetch_all(&self.pool).await?
            .into_iter().map(|r| UsageOperationPoint { operation:r.operation, usage_units:r.usage_units, spend_units:r.spend_units }).collect();
        Ok(UsageReport {
            today_usage_units: today_usage,
            month_usage_units: month_usage,
            month_spend_units: month_spend,
            daily,
            by_operation,
            rows: rows.into_iter().map(Row::into_domain).collect(),
        })
    }
}
