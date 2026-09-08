use std::error::Error;

use application::ports::TenantRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    Tenant,
    shared::ids::{TenantId, UserId},
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresTenantRepository {
    pool: PgPool,
}

impl PostgresTenantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TenantRow {
    id: Uuid,
    name: String,
    owner_user_id: Uuid,
    created_at: DateTime<Utc>,
}

impl TenantRow {
    fn from_domain(tenant: &Tenant) -> Self {
        Self {
            id: tenant.id.0,
            name: tenant.name.clone(),
            owner_user_id: tenant.owner_user_id.0,
            created_at: tenant.created_at,
        }
    }

    fn into_domain(self) -> Tenant {
        Tenant {
            id: TenantId(self.id),
            name: self.name,
            owner_user_id: UserId(self.owner_user_id),
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl TenantRepository for PostgresTenantRepository {
    async fn create(&self, tenant: &Tenant) -> RepoResult<()> {
        let row = TenantRow::from_domain(tenant);
        sqlx::query(
            "INSERT INTO tenants (id, name, owner_user_id, created_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(row.id)
        .bind(row.name)
        .bind(row.owner_user_id)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: TenantId) -> RepoResult<Option<Tenant>> {
        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, owner_user_id, created_at FROM tenants WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(TenantRow::into_domain))
    }

    async fn find_by_owner_user_id(&self, owner_user_id: UserId) -> RepoResult<Option<Tenant>> {
        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, owner_user_id, created_at FROM tenants WHERE owner_user_id = $1",
        )
        .bind(owner_user_id.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(TenantRow::into_domain))
    }

    async fn list_all(&self) -> RepoResult<Vec<Tenant>> {
        let rows = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, owner_user_id, created_at FROM tenants ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(TenantRow::into_domain).collect())
    }
}
