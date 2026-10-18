use std::error::Error;

use application::pagination::{Cursor, Page, PageRequest};
use application::ports::TenantInvitationRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{InvitationId, TenantId, TenantInvitation, shared::types::Role};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresTenantInvitationRepository {
    pool: PgPool,
}

impl PostgresTenantInvitationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TenantInvitationRow {
    id: Uuid,
    tenant_id: Uuid,
    email: String,
    role: String,
    token_prefix: String,
    token_hash: String,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    accepted_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

fn role_to_db(role: &Role) -> &'static str {
    match role {
        Role::Operator => "Operator",
        Role::Admin => "Admin",
        Role::Owner => "Owner",
        Role::Auditor => "Auditor",
        Role::Viewer => "Viewer",
    }
}

fn role_from_db(value: &str) -> RepoResult<Role> {
    match value {
        "Operator" => Ok(Role::Operator),
        "Admin" => Ok(Role::Admin),
        "Owner" => Ok(Role::Owner),
        "Auditor" => Ok(Role::Auditor),
        "Viewer" => Ok(Role::Viewer),
        other => Err(format!("unknown tenant_invitations.role '{other}' in database").into()),
    }
}

impl TenantInvitationRow {
    fn from_domain(invitation: &TenantInvitation) -> Self {
        Self {
            id: invitation.id.into_uuid(),
            tenant_id: invitation.tenant_id.0,
            email: invitation.email.clone(),
            role: role_to_db(&invitation.role).to_string(),
            token_prefix: invitation.token_prefix.clone(),
            token_hash: invitation.token_hash.clone(),
            expires_at: invitation.expires_at,
            revoked_at: invitation.revoked_at,
            accepted_at: invitation.accepted_at,
            created_at: invitation.created_at,
        }
    }

    fn into_domain(self) -> RepoResult<TenantInvitation> {
        Ok(TenantInvitation {
            id: InvitationId::from_uuid(self.id),
            tenant_id: TenantId(self.tenant_id),
            email: self.email,
            role: role_from_db(&self.role)?,
            token_prefix: self.token_prefix,
            token_hash: self.token_hash,
            expires_at: self.expires_at,
            revoked_at: self.revoked_at,
            accepted_at: self.accepted_at,
            created_at: self.created_at,
        })
    }
}

#[async_trait]
impl TenantInvitationRepository for PostgresTenantInvitationRepository {
    async fn create(&self, invitation: &TenantInvitation) -> RepoResult<()> {
        let row = TenantInvitationRow::from_domain(invitation);
        sqlx::query(
            r#"
            INSERT INTO tenant_invitations (
                id, tenant_id, email, role, token_prefix, token_hash,
                expires_at, revoked_at, accepted_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(row.id)
        .bind(row.tenant_id)
        .bind(row.email)
        .bind(row.role)
        .bind(row.token_prefix)
        .bind(row.token_hash)
        .bind(row.expires_at)
        .bind(row.revoked_at)
        .bind(row.accepted_at)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_prefix(&self, token_prefix: &str) -> RepoResult<Option<TenantInvitation>> {
        let row = sqlx::query_as::<_, TenantInvitationRow>(
            r#"
            SELECT id, tenant_id, email, role, token_prefix, token_hash,
                   expires_at, revoked_at, accepted_at, created_at
            FROM tenant_invitations
            WHERE token_prefix = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(token_prefix)
        .fetch_optional(&self.pool)
        .await?;

        row.map(TenantInvitationRow::into_domain).transpose()
    }

    async fn find_by_id(&self, id: InvitationId) -> RepoResult<Option<TenantInvitation>> {
        let row = sqlx::query_as::<_, TenantInvitationRow>(
            r#"
            SELECT id, tenant_id, email, role, token_prefix, token_hash,
                   expires_at, revoked_at, accepted_at, created_at
            FROM tenant_invitations
            WHERE id = $1
            "#,
        )
        .bind(id.into_uuid())
        .fetch_optional(&self.pool)
        .await?;

        row.map(TenantInvitationRow::into_domain).transpose()
    }

    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<TenantInvitation>> {
        // SQL-level cursor pagination; see
        // `PostgresApiKeyRepository::list_by_tenant_paginated` for the
        // full explanation.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, tenant_id, email, role, token_prefix, token_hash, \
                    expires_at, revoked_at, accepted_at, created_at \
             FROM tenant_invitations WHERE tenant_id = ",
        );
        qb.push_bind(tenant_id.0);
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

        let rows = qb
            .build_query_as::<TenantInvitationRow>()
            .fetch_all(&self.pool)
            .await?;

        let items: Vec<TenantInvitation> = rows
            .into_iter()
            .map(TenantInvitationRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Page::from_rows(items, page.limit, |i| {
            Cursor::new(i.created_at, i.id.into_uuid())
        }))
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<TenantInvitation>> {
        let rows = sqlx::query_as::<_, TenantInvitationRow>(
            r#"
            SELECT id, tenant_id, email, role, token_prefix, token_hash,
                   expires_at, revoked_at, accepted_at, created_at
            FROM tenant_invitations
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(TenantInvitationRow::into_domain)
            .collect()
    }

    async fn count_pending_for_tenant(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM tenant_invitations
            WHERE tenant_id = $1
              AND accepted_at IS NULL
              AND revoked_at IS NULL
              AND expires_at > $2
            "#,
        )
        .bind(tenant_id.0)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn mark_revoked(&self, id: InvitationId, revoked_at: DateTime<Utc>) -> RepoResult<()> {
        sqlx::query(
            r#"
            UPDATE tenant_invitations
            SET revoked_at = $2
            WHERE id = $1 AND revoked_at IS NULL AND accepted_at IS NULL
            "#,
        )
        .bind(id.into_uuid())
        .bind(revoked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark_accepted(&self, id: InvitationId, accepted_at: DateTime<Utc>) -> RepoResult<()> {
        sqlx::query(
            r#"
            UPDATE tenant_invitations
            SET accepted_at = $2
            WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL
            "#,
        )
        .bind(id.into_uuid())
        .bind(accepted_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
