use std::error::Error;

use application::pagination::{Cursor, Page, PageRequest};
use application::ports::RumSiteRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::RumSite;
use domain::shared::ids::{RumSiteId, TenantId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresRumSiteRepository {
    pool: PgPool,
}

impl PostgresRumSiteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct RumSiteRow {
    id: Uuid,
    tenant_id: Uuid,
    site_key: String,
    name: String,
    domain: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RumSiteRow {
    fn into_domain(self) -> RumSite {
        RumSite {
            id: RumSiteId(self.id),
            tenant_id: TenantId(self.tenant_id),
            site_key: self.site_key,
            name: self.name,
            domain: self.domain,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[async_trait]
impl RumSiteRepository for PostgresRumSiteRepository {
    async fn create(&self, site: &RumSite) -> RepoResult<()> {
        sqlx::query(
            r#"
            INSERT INTO rum_sites (id, tenant_id, site_key, name, domain, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(site.id.0)
        .bind(site.tenant_id.0)
        .bind(&site.site_key)
        .bind(&site.name)
        .bind(&site.domain)
        .bind(site.created_at)
        .bind(site.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_site_key(&self, site_key: &str) -> RepoResult<Option<RumSite>> {
        let row = sqlx::query_as::<_, RumSiteRow>(
            r#"
            SELECT id, tenant_id, site_key, name, domain, created_at, updated_at
            FROM rum_sites
            WHERE site_key = $1
            "#,
        )
        .bind(site_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(RumSiteRow::into_domain))
    }

    async fn find_by_id(&self, id: RumSiteId, tenant_id: TenantId) -> RepoResult<Option<RumSite>> {
        // Filters on tenant_id in addition to id (rather than trusting the
        // caller to have already checked ownership) so this can never be
        // used as an IDOR primitive even if a caller forgot to check.
        let row = sqlx::query_as::<_, RumSiteRow>(
            r#"
            SELECT id, tenant_id, site_key, name, domain, created_at, updated_at
            FROM rum_sites
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id.0)
        .bind(tenant_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(RumSiteRow::into_domain))
    }

    async fn list_by_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<RumSite>> {
        // SQL-level cursor pagination; see
        // `PostgresApiKeyRepository::list_by_tenant_paginated` for the
        // full explanation of the `peek one ahead` technique and the
        // keyset predicate.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, tenant_id, site_key, name, domain, created_at, updated_at              FROM rum_sites WHERE tenant_id = ",
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
            .build_query_as::<RumSiteRow>()
            .fetch_all(&self.pool)
            .await?;

        let items: Vec<RumSite> = rows.into_iter().map(RumSiteRow::into_domain).collect();

        Ok(Page::from_rows(items, page.limit, |s| {
            Cursor::new(s.created_at, s.id.0)
        }))
    }

    async fn list_by_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<RumSite>> {
        let rows = sqlx::query_as::<_, RumSiteRow>(
            r#"
            SELECT id, tenant_id, site_key, name, domain, created_at, updated_at
            FROM rum_sites
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(RumSiteRow::into_domain).collect())
    }
}
