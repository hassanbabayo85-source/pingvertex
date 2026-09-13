use std::error::Error;

use application::ports::{RumRetentionOutcome, RumRetentionRepository};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use domain::shared::ids::{RumSiteId, TenantId};
use sqlx::PgPool;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresRumRetentionRepository {
    pool: PgPool,
}

impl PostgresRumRetentionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RumRetentionRepository for PostgresRumRetentionRepository {
    async fn run_retention_for_site(
        &self,
        tenant_id: TenantId,
        site_id: RumSiteId,
        raw_retention_days: i64,
        batch_size: u32,
    ) -> RepoResult<RumRetentionOutcome> {
        // Complete-day cutoff: a day is only aggregated once every
        // timestamp in it is older than the retention window. The cutoff
        // is therefore a date, not a timestamp — matching what the hybrid
        // analytics reader below expects (aggregates for `day < cutoff`,
        // raw rows for `created_at >= cutoff midnight UTC`).
        let cutoff_day = (Utc::now() - Duration::days(raw_retention_days)).date_naive();
        let cutoff_midnight = cutoff_day
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always a valid time")
            .and_utc();

        // -----------------------------------------------------------------
        // Step 1: aggregate BEFORE deleting.
        //
        // One INSERT that merges three event streams (page views, JS
        // errors, resource errors) with the session-start stream. Sessions
        // are counted by `started_at` so summing across days gives the
        // same number as the raw `COUNT(*) FROM rum_sessions WHERE
        // started_at IN (...)` query.
        // -----------------------------------------------------------------
        let agg_result = sqlx::query(
            r#"
            WITH ev AS (
                SELECT
                    (created_at AT TIME ZONE 'UTC')::date AS day,
                    COUNT(*) FILTER (WHERE kind = 'pv') AS pv_count,
                    COUNT(*) FILTER (WHERE kind = 'js') AS js_count,
                    COUNT(*) FILTER (WHERE kind = 're') AS re_count,
                    AVG(load_time_ms) FILTER (WHERE kind = 'pv')::int AS avg_load
                FROM (
                    SELECT created_at, 'pv'::text AS kind, load_time_ms
                    FROM rum_page_views
                    WHERE site_id = $1 AND created_at < $2
                    UNION ALL
                    SELECT created_at, 'js'::text AS kind, NULL::integer
                    FROM rum_js_errors
                    WHERE site_id = $1 AND created_at < $2
                    UNION ALL
                    SELECT created_at, 're'::text AS kind, NULL::integer
                    FROM rum_resource_errors
                    WHERE site_id = $1 AND created_at < $2
                ) t
                GROUP BY 1
            ),
            ss AS (
                SELECT
                    (started_at AT TIME ZONE 'UTC')::date AS day,
                    COUNT(*) AS session_count
                FROM rum_sessions
                WHERE site_id = $1 AND started_at < $2
                GROUP BY 1
            )
            INSERT INTO rum_daily_aggregates (
                site_id, tenant_id, day,
                total_sessions, page_view_count, js_error_count, resource_error_count,
                avg_load_time_ms, created_at, updated_at
            )
            SELECT
                $1, $3,
                COALESCE(ev.day, ss.day) AS day,
                COALESCE(ss.session_count, 0),
                COALESCE(ev.pv_count, 0),
                COALESCE(ev.js_count, 0),
                COALESCE(ev.re_count, 0),
                ev.avg_load,
                now(), now()
            FROM ev
            FULL OUTER JOIN ss ON ev.day = ss.day
            WHERE COALESCE(ev.day, ss.day) < $4
            ON CONFLICT (site_id, day) DO UPDATE SET
                total_sessions = EXCLUDED.total_sessions,
                page_view_count = EXCLUDED.page_view_count,
                js_error_count = EXCLUDED.js_error_count,
                resource_error_count = EXCLUDED.resource_error_count,
                avg_load_time_ms = EXCLUDED.avg_load_time_ms,
                updated_at = now()
            "#,
        )
        .bind(site_id.0)
        .bind(cutoff_midnight)
        .bind(tenant_id.0)
        .bind(cutoff_day)
        .execute(&self.pool)
        .await?;

        let days_aggregated = agg_result.rows_affected();

        // -----------------------------------------------------------------
        // Step 2: delete raw rows older than `cutoff_midnight`, batched.
        // -----------------------------------------------------------------
        let mut outcome = RumRetentionOutcome {
            days_aggregated,
            ..Default::default()
        };

        let batch = batch_size as i64;

        outcome.page_views_deleted = self
            .delete_batched("rum_page_views", site_id.0, cutoff_midnight, batch)
            .await?;
        outcome.js_errors_deleted = self
            .delete_batched("rum_js_errors", site_id.0, cutoff_midnight, batch)
            .await?;
        outcome.resource_errors_deleted = self
            .delete_batched("rum_resource_errors", site_id.0, cutoff_midnight, batch)
            .await?;
        outcome.sessions_deleted = self
            .delete_batched("rum_sessions", site_id.0, cutoff_midnight, batch)
            .await?;

        Ok(outcome)
    }
}

impl PostgresRumRetentionRepository {
    /// Deletes up to `batch_size` rows older than `cutoff` for `site_id`
    /// from one of the four raw RUM tables, looping until none remain.
    /// `table` is a compile-time-constant string chosen by the caller, so
    /// the `format!` here is not attacker-reachable.
    async fn delete_batched(
        &self,
        table: &'static str,
        site_id: uuid::Uuid,
        cutoff: chrono::DateTime<Utc>,
        batch_size: i64,
    ) -> RepoResult<u64> {
        // `rum_sessions` has no `created_at`; use `last_seen_at`.
        let ts_column = if table == "rum_sessions" {
            "last_seen_at"
        } else {
            "created_at"
        };

        let sql = format!(
            "DELETE FROM {table}
             WHERE id IN (
                 SELECT id FROM {table}
                 WHERE site_id = $1 AND {ts_column} < $2
                 LIMIT $3
             )"
        );

        let mut total = 0u64;
        loop {
            let result = sqlx::query(&sql)
                .bind(site_id)
                .bind(cutoff)
                .bind(batch_size)
                .execute(&self.pool)
                .await?;
            let deleted = result.rows_affected();
            if deleted == 0 {
                break;
            }
            total += deleted;
        }
        Ok(total)
    }
}
