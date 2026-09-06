use application::ports::AddOnSubscriptionRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{AddOnSubscriptionId, TenantId};
use domain::{AddOnSubscription, AddOnType};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;
type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[derive(Debug, Clone)]
pub struct PostgresAddOnSubscriptionRepository {
    pool: PgPool,
}
impl PostgresAddOnSubscriptionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(Debug, FromRow)]
struct Row {
    id: Uuid,
    tenant_id: Uuid,
    addon_type: String,
    quantity: i64,
    tier: Option<String>,
    price: Decimal,
    currency: String,
    status: String,
    created_at: DateTime<Utc>,
}
impl Row {
    fn into_domain(self) -> Result<AddOnSubscription, Box<dyn Error + Send + Sync>> {
        Ok(AddOnSubscription {
            id: AddOnSubscriptionId(self.id),
            tenant_id: TenantId(self.tenant_id),
            addon_type: self.addon_type.parse().map_err(std::io::Error::other)?,
            quantity: u64::try_from(self.quantity)?,
            tier: self.tier,
            price: self.price,
            currency: self.currency,
            status: self.status.parse().map_err(std::io::Error::other)?,
            created_at: self.created_at,
        })
    }
}
#[async_trait]
impl AddOnSubscriptionRepository for PostgresAddOnSubscriptionRepository {
    async fn create(&self, a: &AddOnSubscription) -> RepoResult<()> {
        sqlx::query("INSERT INTO addon_subscriptions (id,tenant_id,addon_type,quantity,tier,price,currency,status,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(a.id.0).bind(a.tenant_id.0).bind(a.addon_type.as_str()).bind(i64::try_from(a.quantity)?).bind(&a.tier).bind(a.price).bind(&a.currency).bind(a.status.as_str()).bind(a.created_at).execute(&self.pool).await?;
        Ok(())
    }
    async fn find_by_id(&self, id: AddOnSubscriptionId) -> RepoResult<Option<AddOnSubscription>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status,created_at FROM addon_subscriptions WHERE id=$1").bind(id.0).fetch_optional(&self.pool).await?;
        r.map(Row::into_domain).transpose()
    }
    async fn list_active_by_tenant(&self, t: TenantId) -> RepoResult<Vec<AddOnSubscription>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status,created_at FROM addon_subscriptions WHERE tenant_id=$1 AND status='active' ORDER BY created_at DESC").bind(t.0).fetch_all(&self.pool).await?;
        r.into_iter().map(Row::into_domain).collect()
    }
    async fn list_by_tenant_and_type(
        &self,
        t: TenantId,
        k: AddOnType,
    ) -> RepoResult<Vec<AddOnSubscription>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status,created_at FROM addon_subscriptions WHERE tenant_id=$1 AND addon_type=$2 ORDER BY created_at DESC").bind(t.0).bind(k.as_str()).fetch_all(&self.pool).await?;
        r.into_iter().map(Row::into_domain).collect()
    }
}
