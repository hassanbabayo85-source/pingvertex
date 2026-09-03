use std::error::Error;

use application::{
    dto::{
        DashboardOverview, DashboardPerformance, DashboardTrendPoint, IncidentSummary,
        IncidentSummaryData, MonitorOverviewItem, MonitorSummary,
    },
    ports::DashboardRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::CheckStatus;
use domain::shared::ids::TenantId;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresDashboardRepository {
    pool: PgPool,
}

impl PostgresDashboardRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct MonitorWithLatest {
    id: Uuid,
    name: String,
    url: String,
    enabled: bool,
    latest_status: Option<i16>,
    latest_latency_ms: Option<i64>,
    latest_checked_at: Option<DateTime<Utc>>,
    open_incident_id: Option<Uuid>,
    open_incident_started_at: Option<DateTime<Utc>>,
    open_incident_acknowledged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct RecentIncidentRow {
    id: Uuid,
    monitor_id: Uuid,
    started_at: DateTime<Utc>,
    acknowledged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct TrendRow {
    bucket: DateTime<Utc>,
    up_checks: i64,
    checks_count: i64,
    latency_sum_ms: Option<f64>,
    latency_count: i64,
    incident_count: i64,
}

#[async_trait]
impl DashboardRepository for PostgresDashboardRepository {
    async fn get_overview(&self, tenant_id: TenantId) -> RepoResult<DashboardOverview> {
        // Query 1: all monitors with their latest check and open incident
        // (if any) using two LATERAL joins. This replaces the naive N+1
        // pattern of fetching each monitor's checks/incidents individually.
        let monitor_rows = sqlx::query_as::<_, MonitorWithLatest>(
            r#"
            SELECT
                m.id,
                m.name,
                m.url,
                m.enabled,
                lc.status AS latest_status,
                lc.response_time_ms AS latest_latency_ms,
                lc.checked_at AS latest_checked_at,
                oi.id AS open_incident_id,
                oi.started_at AS open_incident_started_at,
                oi.acknowledged_at AS open_incident_acknowledged_at
            FROM monitors m
            LEFT JOIN LATERAL (
                SELECT status, response_time_ms, checked_at
                FROM checks c
                WHERE c.monitor_id = m.id
                ORDER BY c.checked_at DESC
                LIMIT 1
            ) lc ON true
            LEFT JOIN LATERAL (
                SELECT id, started_at, acknowledged_at
                FROM incidents i
                WHERE i.monitor_id = m.id
                  AND i.resolved_at IS NULL
                ORDER BY i.started_at DESC
                LIMIT 1
            ) oi ON true
            WHERE m.tenant_id = $1
            ORDER BY m.created_at DESC
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        // Query 2: 5 most recent incidents (resolved or open) for the tenant.
        let recent_incidents = sqlx::query_as::<_, RecentIncidentRow>(
            r#"
            SELECT
                i.id,
                i.monitor_id,
                i.started_at,
                i.acknowledged_at
            FROM incidents i
            JOIN monitors m ON i.monitor_id = m.id
            WHERE m.tenant_id = $1
            ORDER BY i.started_at DESC
            LIMIT 5
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        // Query 3: fixed-size, tenant-scoped hourly trend for the last 24 hours.
        // `generate_series` preserves empty buckets so the UI can distinguish
        // "no telemetry" from a healthy 100% bucket. This is one bounded query,
        // not a per-monitor/per-bucket loop.
        let trend_rows = sqlx::query_as::<_, TrendRow>(
            r#"
            WITH buckets AS (
                SELECT generate_series(
                    date_trunc('hour', NOW() - INTERVAL '23 hours'),
                    date_trunc('hour', NOW()),
                    INTERVAL '1 hour'
                ) AS bucket
            ),
            check_stats AS (
                SELECT
                    date_trunc('hour', c.checked_at) AS bucket,
                    COUNT(*)::bigint AS checks_count,
                    COUNT(*) FILTER (WHERE c.status = 1)::bigint AS up_checks,
                    SUM(c.response_time_ms)::double precision AS latency_sum_ms,
                    COUNT(c.response_time_ms)::bigint AS latency_count
                FROM checks c
                JOIN monitors m ON m.id = c.monitor_id
                WHERE m.tenant_id = $1
                  AND m.enabled = TRUE
                  AND c.checked_at >= date_trunc('hour', NOW() - INTERVAL '23 hours')
                GROUP BY date_trunc('hour', c.checked_at)
            ),
            incident_stats AS (
                SELECT
                    date_trunc('hour', i.started_at) AS bucket,
                    COUNT(*)::bigint AS incident_count
                FROM incidents i
                JOIN monitors m ON m.id = i.monitor_id
                WHERE m.tenant_id = $1
                  AND i.started_at >= date_trunc('hour', NOW() - INTERVAL '23 hours')
                GROUP BY date_trunc('hour', i.started_at)
            )
            SELECT
                b.bucket,
                COALESCE(cs.up_checks, 0) AS up_checks,
                COALESCE(cs.checks_count, 0) AS checks_count,
                cs.latency_sum_ms,
                COALESCE(cs.latency_count, 0) AS latency_count,
                COALESCE(is_.incident_count, 0) AS incident_count
            FROM buckets b
            LEFT JOIN check_stats cs ON cs.bucket = b.bucket
            LEFT JOIN incident_stats is_ ON is_.bucket = b.bucket
            ORDER BY b.bucket ASC
            "#,
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;

        // Aggregate into response DTO
        let mut monitors = Vec::with_capacity(monitor_rows.len());
        let mut up_count = 0u32;
        let mut down_count = 0u32;
        let mut disabled_count = 0u32;
        let mut open_count = 0u32;

        for row in monitor_rows {
            let status = row.latest_status.map(|s| {
                if s == 1 {
                    CheckStatus::Up
                } else {
                    CheckStatus::Down
                }
            });

            if !row.enabled {
                disabled_count += 1;
            } else if let Some(status) = status {
                match status {
                    CheckStatus::Up => up_count += 1,
                    CheckStatus::Down => down_count += 1,
                }
            }

            if row.open_incident_id.is_some() {
                open_count += 1;
            }

            monitors.push(MonitorOverviewItem {
                id: row.id.to_string(),
                name: row.name,
                url: row.url,
                enabled: row.enabled,
                status,
                latency_ms: row.latest_latency_ms.map(|v| v as u128),
                checked_at: row.latest_checked_at,
                open_incident: row.open_incident_id.map(|incident_id| IncidentSummary {
                    id: incident_id.to_string(),
                    monitor_id: row.id.to_string(),
                    started_at: row.open_incident_started_at.unwrap_or_else(Utc::now),
                    acknowledged_at: row.open_incident_acknowledged_at,
                }),
            });
        }

        let total = monitors.len() as u32;

        let monitor_summary = MonitorSummary {
            total,
            up: up_count,
            down: down_count,
            disabled: disabled_count,
        };

        let mut total_checks = 0u64;
        let mut total_up_checks = 0u64;
        let mut total_latency_sum = 0.0f64;
        let mut total_latency_count = 0u64;

        let trend = trend_rows
            .into_iter()
            .map(|row| {
                let checks_count = u64::try_from(row.checks_count).unwrap_or(0);
                let up_checks = u64::try_from(row.up_checks).unwrap_or(0);
                let latency_count = u64::try_from(row.latency_count).unwrap_or(0);

                total_checks = total_checks.saturating_add(checks_count);
                total_up_checks = total_up_checks.saturating_add(up_checks);
                total_latency_count = total_latency_count.saturating_add(latency_count);
                if let Some(sum) = row.latency_sum_ms {
                    total_latency_sum += sum;
                }

                DashboardTrendPoint {
                    bucket: row.bucket,
                    uptime_percentage: (checks_count > 0)
                        .then(|| (up_checks as f64 / checks_count as f64) * 100.0),
                    avg_latency_ms: (latency_count > 0)
                        .then(|| row.latency_sum_ms.unwrap_or(0.0) / latency_count as f64),
                    checks_count,
                    incident_count: u32::try_from(row.incident_count).unwrap_or(u32::MAX),
                }
            })
            .collect();

        let performance = DashboardPerformance {
            uptime_percentage: (total_checks > 0)
                .then(|| (total_up_checks as f64 / total_checks as f64) * 100.0),
            avg_latency_ms: (total_latency_count > 0)
                .then(|| total_latency_sum / total_latency_count as f64),
            trend,
        };

        let incident_summary = IncidentSummaryData {
            open_count,
            recent: recent_incidents
                .into_iter()
                .map(|r| IncidentSummary {
                    id: r.id.to_string(),
                    monitor_id: r.monitor_id.to_string(),
                    started_at: r.started_at,
                    acknowledged_at: r.acknowledged_at,
                })
                .collect(),
        };

        Ok(DashboardOverview {
            monitor_summary,
            monitors,
            incident_summary,
            performance,
        })
    }
}
