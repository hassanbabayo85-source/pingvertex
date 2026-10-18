use std::error::Error;

use application::pagination::{Cursor, Page, PageRequest};
use application::ports::StatusPageRepository;
use application::ports::monitor_repository::QuotaCheckOutcome;
use async_trait::async_trait;
use domain::entities::{CreateStatusPage, CustomDomainVerification, StatusPage};
use domain::shared::ids::{MonitorId, StatusPageId, TenantId, UserId};
use domain::value_objects::{CustomDomain, HexColor, Subdomain};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed status page repository.
#[derive(Debug, Clone)]
pub struct PostgresStatusPageRepository {
    pool: PgPool,
}

impl PostgresStatusPageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct StatusPageRow {
    id: Uuid,
    user_id: Uuid,
    tenant_id: Uuid,
    name: String,
    subdomain: String,
    custom_domain: Option<String>,
    custom_domain_token: Option<String>,
    custom_domain_requested_at: Option<chrono::DateTime<chrono::Utc>>,
    custom_domain_verified_at: Option<chrono::DateTime<chrono::Utc>>,
    logo_url: Option<String>,
    primary_color: Option<String>,
    monitor_ids: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl StatusPageRow {
    fn from_domain(page: &StatusPage) -> RepoResult<Self> {
        let (
            custom_domain,
            custom_domain_token,
            custom_domain_requested_at,
            custom_domain_verified_at,
        ) = match &page.custom_domain {
            None => (None, None, None, None),
            Some((
                domain,
                CustomDomainVerification::Pending {
                    token,
                    requested_at,
                },
            )) => (
                Some(domain.as_str().to_owned()),
                Some(token.clone()),
                Some(*requested_at),
                None,
            ),
            Some((domain, CustomDomainVerification::Verified { verified_at })) => (
                Some(domain.as_str().to_owned()),
                None,
                None,
                Some(*verified_at),
            ),
        };

        let monitor_ids = serde_json::to_value(
            page.monitor_ids
                .iter()
                .map(|id| id.0)
                .collect::<Vec<Uuid>>(),
        )?;

        Ok(Self {
            id: page.id.0,
            user_id: page.user_id.0,
            tenant_id: page.tenant_id.0,
            name: page.name.clone(),
            subdomain: page.subdomain.as_str().to_owned(),
            custom_domain,
            custom_domain_token,
            custom_domain_requested_at,
            custom_domain_verified_at,
            logo_url: page.logo_url.clone(),
            primary_color: page.primary_color.as_ref().map(|c| c.as_str().to_owned()),
            monitor_ids,
            created_at: page.created_at,
            updated_at: page.updated_at,
        })
    }

    fn into_domain(self) -> RepoResult<StatusPage> {
        let subdomain = Subdomain::new(self.subdomain)?;

        let custom_domain = match (
            self.custom_domain,
            self.custom_domain_token,
            self.custom_domain_requested_at,
            self.custom_domain_verified_at,
        ) {
            (None, _, _, _) => None,
            (Some(domain), Some(token), Some(requested_at), None) => Some((
                CustomDomain::new(domain)?,
                CustomDomainVerification::Pending {
                    token,
                    requested_at,
                },
            )),
            (Some(domain), None, None, Some(verified_at)) => Some((
                CustomDomain::new(domain)?,
                CustomDomainVerification::Verified { verified_at },
            )),
            // The database CHECK constraint (chk_status_pages_custom_domain_state)
            // should make this unreachable, but we fail closed rather than
            // silently guessing a verification state for a row that
            // doesn't match either valid shape.
            _ => {
                return Err("status_pages row has an inconsistent custom_domain state".into());
            }
        };

        let primary_color = self.primary_color.map(HexColor::new).transpose()?;

        let monitor_ids: Vec<Uuid> = serde_json::from_value(self.monitor_ids)?;
        let monitor_ids = monitor_ids.into_iter().map(MonitorId::from).collect();

        let mut page = StatusPage::create(CreateStatusPage {
            id: StatusPageId(self.id),
            user_id: UserId(self.user_id),
            tenant_id: TenantId(self.tenant_id),
            name: self.name,
            subdomain,
            monitor_ids,
        })?;

        // `StatusPage::create` always starts with no custom domain / no
        // logo / no color and stamps fresh timestamps — overwrite those
        // fields with the persisted values now that construction (and its
        // validation) has succeeded.
        page.custom_domain = custom_domain;
        page.logo_url = self.logo_url;
        page.primary_color = primary_color;
        page.created_at = self.created_at;
        page.updated_at = self.updated_at;

        Ok(page)
    }
}

const SELECT_COLUMNS: &str = "id, user_id, tenant_id, name, subdomain, custom_domain, custom_domain_token, \
     custom_domain_requested_at, custom_domain_verified_at, logo_url, primary_color, \
     monitor_ids, created_at, updated_at";

#[async_trait]
impl StatusPageRepository for PostgresStatusPageRepository {
    async fn save(&self, status_page: &StatusPage) -> RepoResult<()> {
        let row = StatusPageRow::from_domain(status_page)?;

        sqlx::query(
            r#"
            INSERT INTO status_pages (
                id,
                user_id,
                tenant_id,
                name,
                subdomain,
                custom_domain,
                custom_domain_token,
                custom_domain_requested_at,
                custom_domain_verified_at,
                logo_url,
                primary_color,
                monitor_ids,
                created_at,
                updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.tenant_id)
        .bind(row.name)
        .bind(row.subdomain)
        .bind(row.custom_domain)
        .bind(row.custom_domain_token)
        .bind(row.custom_domain_requested_at)
        .bind(row.custom_domain_verified_at)
        .bind(row.logo_url)
        .bind(row.primary_color)
        .bind(row.monitor_ids)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update(&self, status_page: &StatusPage) -> RepoResult<()> {
        let row = StatusPageRow::from_domain(status_page)?;

        sqlx::query(
            r#"
            UPDATE status_pages SET
                name = $2,
                subdomain = $3,
                custom_domain = $4,
                custom_domain_token = $5,
                custom_domain_requested_at = $6,
                custom_domain_verified_at = $7,
                logo_url = $8,
                primary_color = $9,
                monitor_ids = $10,
                updated_at = $11
            WHERE id = $1
            "#,
        )
        .bind(row.id)
        .bind(row.name)
        .bind(row.subdomain)
        .bind(row.custom_domain)
        .bind(row.custom_domain_token)
        .bind(row.custom_domain_requested_at)
        .bind(row.custom_domain_verified_at)
        .bind(row.logo_url)
        .bind(row.primary_color)
        .bind(row.monitor_ids)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: StatusPageId) -> RepoResult<Option<StatusPage>> {
        let row = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE id = $1"
        ))
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(StatusPageRow::into_domain).transpose()
    }

    async fn delete(&self, id: StatusPageId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM status_pages WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_for_tenant(&self, id: StatusPageId, tenant_id: TenantId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM status_pages WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(tenant_id.0)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_all_for_tenant_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<StatusPage>> {
        // SQL-level cursor pagination; see
        // `PostgresApiKeyRepository::list_by_tenant_paginated` for the
        // full explanation of the `peek one ahead` technique and the
        // keyset predicate.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE tenant_id = "
        ));
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
            .build_query_as::<StatusPageRow>()
            .fetch_all(&self.pool)
            .await?;

        let items: Vec<StatusPage> = rows
            .into_iter()
            .map(StatusPageRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Page::from_rows(items, page.limit, |sp| {
            Cursor::new(sp.created_at, sp.id.0)
        }))
    }

    async fn find_all_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<StatusPage>> {
        let rows = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE tenant_id = $1 ORDER BY created_at DESC"
        ))
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(StatusPageRow::into_domain).collect()
    }

    async fn delete_for_user(&self, id: StatusPageId, user_id: UserId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM status_pages WHERE id = $1 AND user_id = $2")
            .bind(id.0)
            .bind(user_id.0)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id_for_user(
        &self,
        id: StatusPageId,
        user_id: UserId,
    ) -> RepoResult<Option<StatusPage>> {
        let row = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE id = $1 AND user_id = $2"
        ))
        .bind(id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;

        row.map(StatusPageRow::into_domain).transpose()
    }

    async fn find_all_for_user(&self, user_id: UserId) -> RepoResult<Vec<StatusPage>> {
        let rows = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE user_id = $1 ORDER BY created_at DESC"
        ))
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(StatusPageRow::into_domain).collect()
    }

    /// SECURITY: filters on the exact subdomain string only — no
    /// pattern/LIKE matching — so a request cannot be crafted to match
    /// more than the single intended status page.
    async fn find_by_subdomain(&self, subdomain: &str) -> RepoResult<Option<StatusPage>> {
        let row = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE subdomain = $1"
        ))
        .bind(subdomain)
        .fetch_optional(&self.pool)
        .await?;

        row.map(StatusPageRow::into_domain).transpose()
    }

    /// SECURITY: the `custom_domain_verified_at IS NOT NULL` filter is
    /// load-bearing, not incidental — this is what makes host-header
    /// routing a strict allowlist of *verified* domains only. See the
    /// SECURITY note on this method in the port trait definition and on
    /// `idx_status_pages_verified_custom_domain` in the migration.
    async fn find_by_verified_custom_domain(
        &self,
        custom_domain: &str,
    ) -> RepoResult<Option<StatusPage>> {
        let row = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages \
             WHERE custom_domain = $1 AND custom_domain_verified_at IS NOT NULL"
        ))
        .bind(custom_domain)
        .fetch_optional(&self.pool)
        .await?;

        row.map(StatusPageRow::into_domain).transpose()
    }

    /// Used by the background `CustomDomainVerificationWorker` sweep —
    /// system-wide, not scoped to a user, since the worker acts on behalf
    /// of the whole deployment, not any single caller.
    async fn find_pending_custom_domains(&self) -> RepoResult<Vec<StatusPage>> {
        let rows = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages \
             WHERE custom_domain IS NOT NULL AND custom_domain_verified_at IS NULL"
        ))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(StatusPageRow::into_domain).collect()
    }

    /// Uses the JSONB containment operator (`@>`) against
    /// `monitor_ids` — e.g. `monitor_ids @> '["<uuid>"]'` — to find every
    /// status page (across all tenants) that references `monitor_id`.
    /// No index backs this yet: a GIN index on `monitor_ids` would make this
    /// an index scan instead of a sequential scan, but adding one requires a
    /// new migration, so it is flagged here rather than silently skipped.
    async fn save_if_within_quota(
        &self,
        status_page: &StatusPage,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(format!("status_pages:{}", tenant_id.0))
            .execute(&mut *tx)
            .await?;

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM status_pages WHERE tenant_id = $1")
                .bind(tenant_id.0)
                .fetch_one(&mut *tx)
                .await?;

        if count as u32 >= limit {
            // Drop `tx` without commit — implicit rollback releases the lock.
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }

        let row = StatusPageRow::from_domain(status_page)?;
        sqlx::query(
            r#"
            INSERT INTO status_pages (
                id, user_id, tenant_id, name, subdomain,
                custom_domain, custom_domain_token, custom_domain_requested_at,
                custom_domain_verified_at, logo_url, primary_color, monitor_ids,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.tenant_id)
        .bind(row.name)
        .bind(row.subdomain)
        .bind(row.custom_domain)
        .bind(row.custom_domain_token)
        .bind(row.custom_domain_requested_at)
        .bind(row.custom_domain_verified_at)
        .bind(row.logo_url)
        .bind(row.primary_color)
        .bind(row.monitor_ids)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(QuotaCheckOutcome::Saved)
    }

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM status_pages WHERE tenant_id = $1")
                .bind(tenant_id.0)
                .fetch_one(&self.pool)
                .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn find_all_referencing_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> RepoResult<Vec<StatusPage>> {
        let needle = serde_json::json!([monitor_id.0]);

        let rows = sqlx::query_as::<_, StatusPageRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM status_pages WHERE monitor_ids @> $1"
        ))
        .bind(needle)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(StatusPageRow::into_domain).collect()
    }
}
