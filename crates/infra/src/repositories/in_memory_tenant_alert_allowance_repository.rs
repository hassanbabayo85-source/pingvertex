use application::ports::{
    AlertAllowanceRepoResult as RepoResult, TenantAlertAllowanceRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::entities::TenantAlertAllowance;
use domain::shared::ids::TenantId;
use std::collections::HashMap;
use std::sync::Mutex;

fn cycle_bounds(now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let (y, m) = (now.year(), now.month());
    let start = Utc
        .with_ymd_and_hms(y, m, 1, 0, 0, 0)
        .single()
        .expect("valid month start");
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let end = Utc
        .with_ymd_and_hms(ny, nm, 1, 0, 0, 0)
        .single()
        .expect("valid next month start");
    (start, end)
}

#[derive(Clone, Default)]
pub struct InMemoryTenantAlertAllowanceRepository {
    inner: std::sync::Arc<Mutex<HashMap<TenantId, TenantAlertAllowance>>>,
}

impl InMemoryTenantAlertAllowanceRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TenantAlertAllowanceRepository for InMemoryTenantAlertAllowanceRepository {
    async fn current_cycle(
        &self,
        tenant_id: TenantId,
        enabled_monitor_count: u64,
        now: DateTime<Utc>,
    ) -> RepoResult<TenantAlertAllowance> {
        let (cycle_start, cycle_end) = cycle_bounds(now);
        let sms_limit = TenantAlertAllowance::compute_sms_limit(enabled_monitor_count);

        let mut guard = self.inner.lock().unwrap();
        let entry = guard.entry(tenant_id).or_insert_with(|| TenantAlertAllowance {
            tenant_id,
            cycle_started_at: cycle_start,
            cycle_ends_at: cycle_end,
            sms_limit,
            sms_used: 0,
            voice_limit: 0,
            voice_used: 0,
            phone_numbers_limit: 0,
            phone_numbers_used: 0,
            created_at: now,
            updated_at: now,
        });
        if entry.cycle_started_at < cycle_start {
            entry.cycle_started_at = cycle_start;
            entry.cycle_ends_at = cycle_end;
            entry.sms_limit = sms_limit;
            entry.sms_used = 0;
            entry.voice_used = 0;
            entry.phone_numbers_used = 0;
            entry.updated_at = now;
        }
        Ok(entry.clone())
    }

    async fn try_consume_sms(
        &self,
        tenant_id: TenantId,
        now: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let mut guard = self.inner.lock().unwrap();
        let Some(entry) = guard.get_mut(&tenant_id) else {
            return Ok(false);
        };
        if entry.cycle_started_at > now || entry.cycle_ends_at <= now {
            return Ok(false);
        }
        if entry.sms_used >= entry.sms_limit {
            return Ok(false);
        }
        entry.sms_used += 1;
        entry.updated_at = now;
        Ok(true)
    }

    async fn find(&self, tenant_id: TenantId) -> RepoResult<Option<TenantAlertAllowance>> {
        let guard = self.inner.lock().unwrap();
        Ok(guard.get(&tenant_id).cloned())
    }
}
