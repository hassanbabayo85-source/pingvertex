use std::error::Error;

use application::ports::MaintenanceWindowRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::MaintenanceWindow;
use domain::shared::ids::{MaintenanceWindowId, MonitorId, TenantId, UserId};
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

    async fn delete_for_user(&self, id: MaintenanceWindowId, user_id: UserId) -> RepoResult<bool> {
        let result = sqlx::query(
            "DELETE FROM maintenance_windows \
             WHERE id = $1 AND monitor_id IN (SELECT id FROM monitors WHERE user_id = $2)",
        )
        .bind(id.0)
        .bind(user_id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id_for_user(
        &self,
        id: MaintenanceWindowId,
        user_id: UserId,
    ) -> RepoResult<Option<MaintenanceWindow>> {
        let row = sqlx::query_as::<_, MaintenanceWindowRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM maintenance_windows \
             WHERE id = $1 AND monitor_id IN (SELECT id FROM monitors WHERE id = maintenance_windows.monitor_id AND user_id = $2)"
        ))
        .bind(id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(MaintenanceWindowRow::into_domain))
    }

    async fn list_by_monitor_for_user(
        &self,
        monitor_id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Vec<MaintenanceWindow>>> {
        let owns_monitor: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM monitors WHERE id = $1 AND user_id = $2)",
        )
        .bind(monitor_id.0)
        .bind(user_id.0)
        .fetch_one(&self.pool)
        .await?;

        if !owns_monitor {
            return Ok(None);
        }

        let rows = sqlx::query_as::<_, MaintenanceWindowRow>(&format!(
            "SELECT {SELECT_COLUMNS} FROM maintenance_windows \
             WHERE monitor_id = $1 ORDER BY starts_at DESC"
        ))
        .bind(monitor_id.0)
        .fetch_all(&self.pool)
        .await?;

        let windows = rows
            .into_iter()
            .map(MaintenanceWindowRow::into_domain)
            .collect::<Vec<_>>();

        Ok(Some(windows))
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
}
