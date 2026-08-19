use std::error::Error;

use application::ports::TenantMembershipRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    TenantMembership,
    shared::{
        ids::{TenantId, UserId},
        types::Role,
    },
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresTenantMembershipRepository {
    pool: PgPool,
}

impl PostgresTenantMembershipRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TenantMembershipRow {
    tenant_id: Uuid,
    user_id: Uuid,
    role: String,
    invited_at: DateTime<Utc>,
    joined_at: Option<DateTime<Utc>>,
}

fn role_to_db(role: &Role) -> &'static str {
    match role {
        Role::Member => "Member",
        Role::Admin => "Admin",
        Role::Owner => "Owner",
        Role::Auditor => "Auditor",
    }
}

fn role_from_db(value: &str) -> RepoResult<Role> {
    match value {
        "Member" => Ok(Role::Member),
        "Admin" => Ok(Role::Admin),
        "Owner" => Ok(Role::Owner),
        "Auditor" => Ok(Role::Auditor),
        other => Err(format!("unknown tenant_memberships.role '{other}' in database").into()),
    }
}

impl TenantMembershipRow {
    fn from_domain(membership: &TenantMembership) -> Self {
        Self {
            tenant_id: membership.tenant_id.0,
            user_id: membership.user_id.0,
            role: role_to_db(&membership.role).to_string(),
            invited_at: membership.invited_at,
            joined_at: membership.joined_at,
        }
    }

    fn into_domain(self) -> RepoResult<TenantMembership> {
        Ok(TenantMembership {
            tenant_id: TenantId(self.tenant_id),
            user_id: UserId(self.user_id),
            role: role_from_db(&self.role)?,
            invited_at: self.invited_at,
            joined_at: self.joined_at,
        })
    }
}

#[async_trait]
impl TenantMembershipRepository for PostgresTenantMembershipRepository {
    async fn create(&self, membership: &TenantMembership) -> RepoResult<()> {
        let row = TenantMembershipRow::from_domain(membership);
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(row.tenant_id)
        .bind(row.user_id)
        .bind(row.role)
        .bind(row.invited_at)
        .bind(row.joined_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_tenant_and_user(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Option<TenantMembership>> {
        let row = sqlx::query_as::<_, TenantMembershipRow>(
            "SELECT tenant_id, user_id, role, invited_at, joined_at \
             FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(TenantMembershipRow::into_domain).transpose()
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantMembership>> {
        let rows = sqlx::query_as::<_, TenantMembershipRow>(
            "SELECT tenant_id, user_id, role, invited_at, joined_at \
             FROM tenant_memberships WHERE tenant_id = $1 ORDER BY invited_at ASC",
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(TenantMembershipRow::into_domain)
            .collect()
    }

    async fn list_by_user(&self, user_id: UserId) -> RepoResult<Vec<TenantMembership>> {
        let rows = sqlx::query_as::<_, TenantMembershipRow>(
            "SELECT tenant_id, user_id, role, invited_at, joined_at \
             FROM tenant_memberships WHERE user_id = $1 ORDER BY invited_at ASC",
        )
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(TenantMembershipRow::into_domain)
            .collect()
    }

    async fn update_role(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        role: Role,
    ) -> RepoResult<()> {
        sqlx::query(
            "UPDATE tenant_memberships SET role = $3 WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id.0)
        .bind(user_id.0)
        .bind(role_to_db(&role))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
