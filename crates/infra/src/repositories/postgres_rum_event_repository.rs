use std::error::Error;

use application::ports::{RumEventRepository, UpsertedRumSession};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{RumSessionId, RumSiteId, TenantId};
use domain::{
    DeviceType, RumJsError, RumPageView, RumResourceError, RumResourceType, RumSession, RumVitals,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresRumEventRepository {
    pool: PgPool,
}

impl PostgresRumEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn device_type_to_str(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::Desktop => "desktop",
        DeviceType::Mobile => "mobile",
        DeviceType::Tablet => "tablet",
        DeviceType::Other => "other",
    }
}

fn device_type_from_str(value: Option<&str>) -> Option<DeviceType> {
    match value {
        Some("desktop") => Some(DeviceType::Desktop),
        Some("mobile") => Some(DeviceType::Mobile),
        Some("tablet") => Some(DeviceType::Tablet),
        Some(_) => Some(DeviceType::Other),
        None => None,
    }
}

fn resource_type_to_str(resource_type: RumResourceType) -> &'static str {
    match resource_type {
        RumResourceType::Script => "script",
        RumResourceType::Stylesheet => "stylesheet",
        RumResourceType::Image => "image",
        RumResourceType::Fetch => "fetch",
        RumResourceType::Other => "other",
    }
}

#[derive(Debug, Clone, FromRow)]
struct RumSessionRow {
    id: Uuid,
    tenant_id: Uuid,
    site_id: Uuid,
    session_key: String,
    browser_name: Option<String>,
    browser_version: Option<String>,
    os_name: Option<String>,
    device_type: Option<String>,
    country_code: Option<String>,
    started_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    /// Postgres system column: `0` on a fresh INSERT, non-zero when the
    /// `ON CONFLICT DO UPDATE` path fired. Used to report `created`.
    is_new: bool,
}

impl RumSessionRow {
    fn into_domain(self) -> RumSession {
        RumSession {
            id: RumSessionId(self.id),
            tenant_id: TenantId(self.tenant_id),
            site_id: RumSiteId(self.site_id),
            session_key: self.session_key,
            browser_name: self.browser_name,
            browser_version: self.browser_version,
            os_name: self.os_name,
            device_type: device_type_from_str(self.device_type.as_deref()),
            country_code: self.country_code,
            started_at: self.started_at,
            last_seen_at: self.last_seen_at,
        }
    }
}

#[async_trait]
impl RumEventRepository for PostgresRumEventRepository {
    async fn upsert_session(
        &self,
        session: &RumSession,
        now: DateTime<Utc>,
    ) -> RepoResult<UpsertedRumSession> {
        // GAP 3 Kashi E: a beacon either extends an active 30-minute
        // session (no new charge) or starts a new one (billed once).
        //
        // Single-statement upsert. The CASE clauses decide between
        // "extend" and "restart" atomically; RETURNING compares the
        // resulting `started_at` with the caller-supplied timestamp to
        // report `is_new`.
        //
        //   * fresh INSERT                     -> started_at = $10, is_new = true
        //   * existing row, last_seen within 30m -> CASE keeps OLD started_at, is_new = false
        //   * existing row, last_seen > 30m ago  -> CASE sets started_at = $10, is_new = true
        let cutoff = now - chrono::Duration::minutes(RumSession::SESSION_WINDOW_MINUTES);

        let row = sqlx::query_as::<_, RumSessionRow>(
            r#"
            INSERT INTO rum_sessions (
                id, tenant_id, site_id, session_key,
                browser_name, browser_version, os_name, device_type, country_code,
                started_at, last_seen_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
            ON CONFLICT (site_id, session_key) DO UPDATE SET
                last_seen_at = EXCLUDED.last_seen_at,
                started_at = CASE
                    WHEN rum_sessions.last_seen_at < $11 THEN EXCLUDED.started_at
                    ELSE rum_sessions.started_at
                END,
                browser_name = CASE
                    WHEN rum_sessions.last_seen_at < $11 THEN EXCLUDED.browser_name
                    ELSE rum_sessions.browser_name
                END,
                browser_version = CASE
                    WHEN rum_sessions.last_seen_at < $11 THEN EXCLUDED.browser_version
                    ELSE rum_sessions.browser_version
                END,
                os_name = CASE
                    WHEN rum_sessions.last_seen_at < $11 THEN EXCLUDED.os_name
                    ELSE rum_sessions.os_name
                END,
                device_type = CASE
                    WHEN rum_sessions.last_seen_at < $11 THEN EXCLUDED.device_type
                    ELSE rum_sessions.device_type
                END,
                country_code = CASE
                    WHEN rum_sessions.last_seen_at < $11 THEN EXCLUDED.country_code
                    ELSE rum_sessions.country_code
                END
            RETURNING
                id, tenant_id, site_id, session_key,
                browser_name, browser_version, os_name, device_type, country_code,
                started_at, last_seen_at,
                (started_at = $10) AS is_new
            "#,
        )
        .bind(session.id.0)
        .bind(session.tenant_id.0)
        .bind(session.site_id.0)
        .bind(&session.session_key)
        .bind(&session.browser_name)
        .bind(&session.browser_version)
        .bind(&session.os_name)
        .bind(session.device_type.map(device_type_to_str))
        .bind(&session.country_code)
        .bind(session.started_at)
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await?;

        let is_new = row.is_new;
        Ok(UpsertedRumSession {
            session: row.into_domain(),
            created: is_new,
        })
    }

    async fn insert_page_view(&self, page_view: &RumPageView) -> RepoResult<()> {
        let RumVitals {
            ttfb_ms,
            fcp_ms,
            lcp_ms,
            inp_ms,
            cls,
            load_time_ms,
        } = page_view.vitals;

        sqlx::query(
            r#"
            INSERT INTO rum_page_views (
                id, tenant_id, site_id, session_id, path, referrer_path,
                ttfb_ms, fcp_ms, lcp_ms, inp_ms, cls, load_time_ms, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(page_view.id.0)
        .bind(page_view.tenant_id.0)
        .bind(page_view.site_id.0)
        .bind(page_view.session_id.0)
        .bind(&page_view.path)
        .bind(&page_view.referrer_path)
        .bind(ttfb_ms)
        .bind(fcp_ms)
        .bind(lcp_ms)
        .bind(inp_ms)
        .bind(cls)
        .bind(load_time_ms)
        .bind(page_view.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn insert_js_error(&self, error: &RumJsError) -> RepoResult<()> {
        sqlx::query(
            r#"
            INSERT INTO rum_js_errors (
                id, tenant_id, site_id, session_id, page_view_id,
                message, source, line_number, column_number, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(error.id.0)
        .bind(error.tenant_id.0)
        .bind(error.site_id.0)
        .bind(error.session_id.0)
        .bind(error.page_view_id.map(|id| id.0))
        .bind(&error.message)
        .bind(&error.source)
        .bind(error.line_number)
        .bind(error.column_number)
        .bind(error.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn insert_resource_error(&self, error: &RumResourceError) -> RepoResult<()> {
        sqlx::query(
            r#"
            INSERT INTO rum_resource_errors (
                id, tenant_id, site_id, session_id, page_view_id,
                resource_url, resource_type, status_code, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(error.id.0)
        .bind(error.tenant_id.0)
        .bind(error.site_id.0)
        .bind(error.session_id.0)
        .bind(error.page_view_id.map(|id| id.0))
        .bind(&error.resource_url)
        .bind(error.resource_type.map(resource_type_to_str))
        .bind(error.status_code)
        .bind(error.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
