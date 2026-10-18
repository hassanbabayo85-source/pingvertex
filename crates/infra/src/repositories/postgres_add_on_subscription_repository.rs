use application::pagination::{Cursor, Page, PageRequest};
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
    provider_checkout_id: Option<String>,
    paid_at: Option<DateTime<Utc>>,
    canceled_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
            provider_checkout_id: self.provider_checkout_id,
            paid_at: self.paid_at,
            canceled_at: self.canceled_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
#[async_trait]
impl AddOnSubscriptionRepository for PostgresAddOnSubscriptionRepository {
    async fn create(&self, a: &AddOnSubscription) -> RepoResult<()> {
        sqlx::query("INSERT INTO addon_subscriptions (id,tenant_id,addon_type,quantity,tier,price,currency,status,provider_checkout_id,paid_at,canceled_at,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(a.id.0)
            .bind(a.tenant_id.0)
            .bind(a.addon_type.as_str())
            .bind(i64::try_from(a.quantity)?)
            .bind(&a.tier)
            .bind(a.price)
            .bind(&a.currency)
            .bind(a.status.as_str())
            .bind(&a.provider_checkout_id)
            .bind(a.paid_at)
            .bind(a.canceled_at)
            .bind(a.created_at)
            .bind(a.updated_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    async fn find_by_id(&self, id: AddOnSubscriptionId) -> RepoResult<Option<AddOnSubscription>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status,provider_checkout_id,paid_at,canceled_at,created_at,updated_at FROM addon_subscriptions WHERE id=$1").bind(id.0).fetch_optional(&self.pool).await?;
        r.map(Row::into_domain).transpose()
    }
    async fn list_active_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<AddOnSubscription>> {
        // SQL-level cursor pagination.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status, \
                    provider_checkout_id,paid_at,canceled_at,created_at,updated_at \
             FROM addon_subscriptions WHERE tenant_id = ",
        );
        qb.push_bind(tenant_id.0);
        qb.push(" AND status = 'active'");
        if let Some(c) = page.cursor {
            qb.push(" AND (created_at < ")
                .push_bind(c.created_at)
                .push(" OR (created_at = ")
                .push_bind(c.created_at)
                .push(" AND id < ")
                .push_bind(c.id)
                .push("))");
        }
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(page.fetch_size());

        let rows = qb.build_query_as::<Row>().fetch_all(&self.pool).await?;

        let items: Vec<AddOnSubscription> = rows
            .into_iter()
            .map(Row::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Page::from_rows(items, page.limit, |a| {
            Cursor::new(a.created_at, a.id.0)
        }))
    }

    async fn list_by_tenant_and_type_paginated(
        &self,
        tenant_id: TenantId,
        addon_type: AddOnType,
        page: PageRequest,
    ) -> RepoResult<Page<AddOnSubscription>> {
        // SQL-level cursor pagination.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status, \
                    provider_checkout_id,paid_at,canceled_at,created_at,updated_at \
             FROM addon_subscriptions WHERE tenant_id = ",
        );
        qb.push_bind(tenant_id.0);
        qb.push(" AND addon_type = ").push_bind(addon_type.as_str());
        if let Some(c) = page.cursor {
            qb.push(" AND (created_at < ")
                .push_bind(c.created_at)
                .push(" OR (created_at = ")
                .push_bind(c.created_at)
                .push(" AND id < ")
                .push_bind(c.id)
                .push("))");
        }
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(page.fetch_size());

        let rows = qb.build_query_as::<Row>().fetch_all(&self.pool).await?;

        let items: Vec<AddOnSubscription> = rows
            .into_iter()
            .map(Row::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Page::from_rows(items, page.limit, |a| {
            Cursor::new(a.created_at, a.id.0)
        }))
    }

    async fn list_active_by_tenant(&self, t: TenantId) -> RepoResult<Vec<AddOnSubscription>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status,provider_checkout_id,paid_at,canceled_at,created_at,updated_at FROM addon_subscriptions WHERE tenant_id=$1 AND status='active' ORDER BY created_at DESC").bind(t.0).fetch_all(&self.pool).await?;
        r.into_iter().map(Row::into_domain).collect()
    }
    async fn list_by_tenant_and_type(
        &self,
        t: TenantId,
        k: AddOnType,
    ) -> RepoResult<Vec<AddOnSubscription>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,addon_type,quantity,tier,price,currency,status,provider_checkout_id,paid_at,canceled_at,created_at,updated_at FROM addon_subscriptions WHERE tenant_id=$1 AND addon_type=$2 ORDER BY created_at DESC").bind(t.0).bind(k.as_str()).fetch_all(&self.pool).await?;
        r.into_iter().map(Row::into_domain).collect()
    }

    async fn update(&self, a: &AddOnSubscription) -> RepoResult<()> {
        sqlx::query(
            "UPDATE addon_subscriptions
             SET addon_type=$2,
                 quantity=$3,
                 tier=$4,
                 price=$5,
                 currency=$6,
                 status=$7,
                 provider_checkout_id=$8,
                 paid_at=$9,
                 canceled_at=$10,
                 updated_at=$11
             WHERE id=$1 AND tenant_id=$12",
        )
        .bind(a.id.0)
        .bind(a.addon_type.as_str())
        .bind(i64::try_from(a.quantity)?)
        .bind(&a.tier)
        .bind(a.price)
        .bind(&a.currency)
        .bind(a.status.as_str())
        .bind(&a.provider_checkout_id)
        .bind(a.paid_at)
        .bind(a.canceled_at)
        .bind(a.updated_at)
        .bind(a.tenant_id.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
