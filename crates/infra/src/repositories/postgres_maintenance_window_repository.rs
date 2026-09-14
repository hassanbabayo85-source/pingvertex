use std::error::Error;

use application::pagination::{Cursor, Page, PageRequest};
use application::ports::MaintenanceWindowRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, TenantId};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresMaintenanceWindowRepository {
    pool: PgPool,
}

impl PostgresMaintenanceWindowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct MaintenanceWindowRow {
    id: Uuid,
    monitor_id: Uuid,
    tenant_id: Uuid,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    reason: String,
    created_at: DateTime<Utc>,
}

impl MaintenanceWindowRow {
    fn from_domain(window: &MaintenanceWindow) -> Self {
        Self {
            id: window.id.0,
            monitor_id: window.monitor_id.0,
            tenant_id: window.tenant_id.0,
            starts_at: window.starts_at,
            ends_at: window.ends_at,
            reason: window.reason.clone(),
            created_at: window.created_at,
        }
    }

    fn into_domain(self) -> MaintenanceWindow {
        MaintenanceWindow {
            id: MaintenanceWindowId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            tenant_id: TenantId(self.tenant_id),
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            reason: self.reason,
            created_at: self.created_at,
        }
    }
}

const SELECT_COLUMNS: &str = "id, monitor_id, tenant_id, starts_at, ends_at, reason, created_at";

#[async_trait]
impl MaintenanceWindowRepository for PostgresMaintenanceWindowRepository {
    async fn create(&self, window: &MaintenanceWindow) -> RepoResult<()> {
        let row = MaintenanceWindowRow::from_domain(window);

        sqlx::query(
            r#"
            INSERT INTO maintenance_windows (
                id, monitor_id, tenant_id, starts_at, ends_at, reason, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(row.id)
        .bind(row.monitor_id)
        .bind(row.tenant_id)
        .bind(row.starts_at)
        .bind(row.ends_at)
        .bind(row.reason)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_active_by_monitor(
        &self,
        monitor_id: MonitorId,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        let rows = sqlx::query_as::<_, MaintenanceWindowRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM maintenance_windows \
             WHERE monitor_id = $1 AND starts_at <= $2 AND ends_at >= $2 \
             ORDER BY starts_at DESC"
        ))
        .bind(monitor_id.0)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(MaintenanceWindowRow::into_domain)
            .collect())
    }

    async fn delete_for_tenant(
        &self,
        id: MaintenanceWindowId,
        tenant_id: TenantId,
    ) -> RepoResult<bool> {
        let result =
            sqlx::query("DELETE FROM maintenance_windows WHERE id = $1 AND tenant_id = $2")
                .bind(id.0)
                .bind(tenant_id.0)
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_by_monitor_paginated(
        &self,
        monitor_id: MonitorId,
        page: PageRequest,
    ) -> RepoResult<Page<MaintenanceWindow>> {
        // PART 8 — SQL-level cursor pagination. Note: `MaintenanceWindowRow`
        // has no `updated_at`; the cursor still orders by `(created_at, id)`
        // which is stable because `created_at` is set at insert and `id`
        // is immutable.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(format!(
            "SELECT {SELECT_COLUMNS} FROM maintenance_windows WHERE monitor_id = "
        ));
        qb.push_bind(monitor_id.0);
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
            .build_query_as::<MaintenanceWindowRow>()
            .fetch_all(&self.pool)
            .await?;

        // `MaintenanceWindowRow::into_domain` returns `MaintenanceWindow`
        // directly (not `Result`), so no collect::<RepoResult<...>>.
        let items: Vec<MaintenanceWindow> = rows
            .into_iter()
            .map(MaintenanceWindowRow::into_domain)
            .collect();

        Ok(Page::from_rows(items, page.limit, |w| {
            Cursor::new(w.created_at, w.id.0)
        }))
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<MaintenanceWindow>> {
        let rows = sqlx::query_as::<_, MaintenanceWindowRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM maintenance_windows \
             WHERE monitor_id = $1 ORDER BY starts_at DESC"
        ))
        .bind(monitor_id.0)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(MaintenanceWindowRow::into_domain)
            .collect())
    }
}
