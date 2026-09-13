use application::ports::{
    AlertAllowanceRepoResult as RepoResult, TenantAlertAllowanceRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::entities::TenantAlertAllowance;
use domain::shared::ids::TenantId;
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone)]
pub struct PostgresTenantAlertAllowanceRepository {
    pool: PgPool,
}

impl PostgresTenantAlertAllowanceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct Row {
    tenant_id: uuid::Uuid,
    cycle_started_at: DateTime<Utc>,
    cycle_ends_at: DateTime<Utc>,
    sms_limit: i32,
    sms_used: i32,
    voice_limit: i32,
    voice_used: i32,
    phone_numbers_limit: i32,
    phone_numbers_used: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Row {
    fn into_domain(self) -> TenantAlertAllowance {
        TenantAlertAllowance {
            tenant_id: TenantId(self.tenant_id),
            cycle_started_at: self.cycle_started_at,
            cycle_ends_at: self.cycle_ends_at,
            sms_limit: self.sms_limit,
            sms_used: self.sms_used,
            voice_limit: self.voice_limit,
            voice_used: self.voice_used,
            phone_numbers_limit: self.phone_numbers_limit,
            phone_numbers_used: self.phone_numbers_used,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// UTC cycle boundaries [1st-of-month 00:00, 1st-of-next-month 00:00).
pub(crate) fn current_cycle_bounds(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let year = now.year();
    let month = now.month();
    let start = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .expect("valid month start");
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = Utc
        .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
        .single()
        .expect("valid next month start");
    (start, end)
}

const SELECT_COLUMNS: &str = "tenant_id, cycle_started_at, cycle_ends_at, sms_limit, sms_used, voice_limit, voice_used, phone_numbers_limit, phone_numbers_used, created_at, updated_at";

#[async_trait]
impl TenantAlertAllowanceRepository for PostgresTenantAlertAllowanceRepository {
    async fn current_cycle(
        &self,
        tenant_id: TenantId,
        enabled_monitor_count: u64,
        now: DateTime<Utc>,
    ) -> RepoResult<TenantAlertAllowance> {
        let (cycle_start, cycle_end) = current_cycle_bounds(now);
        let sms_limit = TenantAlertAllowance::compute_sms_limit(enabled_monitor_count);

        let mut tx = self.pool.begin().await?;

        // 1. Create an empty row if the tenant has never had one.
        sqlx::query(
            "INSERT INTO tenant_alert_allowances
                 (tenant_id, cycle_started_at, cycle_ends_at, sms_limit)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (tenant_id) DO NOTHING",
        )
        .bind(tenant_id.0)
        .bind(cycle_start)
        .bind(cycle_end)
        .bind(sms_limit)
        .execute(&mut *tx)
        .await?;

        // 2. Lazy-reset if the stored cycle is stale. Only touches rows
        //    whose `cycle_started_at` is older than the current cycle
        //    start; if the row is already current, the WHERE clause
        //    matches no rows and nothing changes.
        sqlx::query(
            "UPDATE tenant_alert_allowances
             SET cycle_started_at = $2,
                 cycle_ends_at = $3,
                 sms_limit = $4,
                 sms_used = 0,
                 voice_used = 0,
                 phone_numbers_used = 0,
                 updated_at = now()
             WHERE tenant_id = $1 AND cycle_started_at < $2",
        )
        .bind(tenant_id.0)
        .bind(cycle_start)
        .bind(cycle_end)
        .bind(sms_limit)
        .execute(&mut *tx)
        .await?;

        // 3. Read the row back.
        let q = format!(
            "SELECT {SELECT_COLUMNS} FROM tenant_alert_allowances WHERE tenant_id = $1"
        );
        let row = sqlx::query_as::<_, Row>(&q)
            .bind(tenant_id.0)
            .fetch_one(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(row.into_domain())
    }

    async fn try_consume_sms(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let result = sqlx::query(
            "UPDATE tenant_alert_allowances
             SET sms_used = sms_used + 1, updated_at = now()
             WHERE tenant_id = $1
               AND cycle_started_at <= $2
               AND cycle_ends_at > $2
               AND sms_used < sms_limit",
        )
        .bind(tenant_id.0)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn find(&self, tenant_id: TenantId) -> RepoResult<Option<TenantAlertAllowance>> {
        let q = format!(
            "SELECT {SELECT_COLUMNS} FROM tenant_alert_allowances WHERE tenant_id = $1"
        );
        let row = sqlx::query_as::<_, Row>(&q)
            .bind(tenant_id.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(Row::into_domain))
    }
}
