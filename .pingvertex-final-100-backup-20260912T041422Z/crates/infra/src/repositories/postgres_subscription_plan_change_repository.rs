use application::ports::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{SubscriptionPlanChange, SubscriptionPlanChangeStatus};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
pub struct PostgresSubscriptionPlanChangeRepository {
    pool: PgPool,
}
impl PostgresSubscriptionPlanChangeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(FromRow)]
struct Row {
    id: Uuid,
    subscription_id: Uuid,
    tenant_id: Uuid,
    old_plan: String,
    new_plan: String,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    calculated_amount: Decimal,
    currency: String,
    pricing_version: i32,
    status: String,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}
fn dom(r: Row) -> Result<SubscriptionPlanChange, Box<dyn std::error::Error + Send + Sync>> {
    Ok(SubscriptionPlanChange {
        id: r.id,
        subscription_id: r.subscription_id,
        tenant_id: r.tenant_id,
        old_plan: r.old_plan.parse()?,
        new_plan: r.new_plan.parse()?,
        period_start: r.period_start,
        period_end: r.period_end,
        calculated_amount: r.calculated_amount,
        currency: r.currency,
        pricing_version: r.pricing_version as u32,
        status: match r.status.as_str() {
            "pending" => SubscriptionPlanChangeStatus::Pending,
            "completed" => SubscriptionPlanChangeStatus::Completed,
            "failed" => SubscriptionPlanChangeStatus::Failed,
            _ => return Err("invalid plan change status".into()),
        },
        created_at: r.created_at,
        completed_at: r.completed_at,
    })
}
#[async_trait]
impl SubscriptionPlanChangeRepository for PostgresSubscriptionPlanChangeRepository {
    async fn create(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()> {
        sqlx::query("INSERT INTO subscription_plan_changes(id,subscription_id,tenant_id,old_plan,new_plan,period_start,period_end,calculated_amount,currency,pricing_version,status,created_at,completed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(c.id).bind(c.subscription_id).bind(c.tenant_id).bind(c.old_plan.as_str()).bind(c.new_plan.as_str()).bind(c.period_start).bind(c.period_end).bind(c.calculated_amount).bind(&c.currency).bind(c.pricing_version as i32).bind(c.status.as_str()).bind(c.created_at).bind(c.completed_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn update(&self, c: &SubscriptionPlanChange) -> PlanChangeRepoResult<()> {
        sqlx::query("UPDATE subscription_plan_changes SET status=$2,completed_at=$3 WHERE id=$1 AND tenant_id=$4").bind(c.id).bind(c.status.as_str()).bind(c.completed_at).bind(c.tenant_id).execute(&self.pool).await?;
        Ok(())
    }
    async fn find_by_id(
        &self,
        id: Uuid,
        t: Uuid,
    ) -> PlanChangeRepoResult<Option<SubscriptionPlanChange>> {
        Ok(sqlx::query_as::<_,Row>("SELECT id,subscription_id,tenant_id,old_plan,new_plan,period_start,period_end,calculated_amount,currency,pricing_version,status,created_at,completed_at FROM subscription_plan_changes WHERE id=$1 AND tenant_id=$2").bind(id).bind(t).fetch_optional(&self.pool).await?.map(dom).transpose()?)
    }
}
