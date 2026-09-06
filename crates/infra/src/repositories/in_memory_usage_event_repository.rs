use application::ports::{
    UsageDailyPoint, UsageEventRepoResult, UsageEventRepository, UsageOperationPoint, UsageReport,
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use domain::UsageEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct InMemoryUsageEventRepository {
    rows: Arc<Mutex<HashMap<(Uuid, String), UsageEvent>>>,
}
#[async_trait]
impl UsageEventRepository for InMemoryUsageEventRepository {
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&(tenant_id, request_id.to_owned()))
            .cloned())
    }
    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        let mut rows = self.rows.lock().unwrap();
        let key = (event.tenant_id, event.request_id.clone());
        if let Some(existing) = rows.get(&key) {
            return Ok(existing.clone());
        }
        rows.insert(key, event.clone());
        Ok(event.clone())
    }
    async fn report(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        operation: Option<&str>,
        status: Option<&str>,
        limit: u32,
    ) -> UsageEventRepoResult<UsageReport> {
        let rows = self
            .rows
            .lock()
            .unwrap()
            .values()
            .filter(|e| {
                e.tenant_id == tenant_id
                    && e.created_at >= from
                    && e.created_at < to
                    && operation.map_or(true, |v| v == e.operation)
                    && status.map_or(true, |v| v == e.status)
            })
            .cloned()
            .collect::<Vec<_>>();
        let today_start = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let month_start = Utc::now()
            .date_naive()
            .with_day(1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let today_usage = rows
            .iter()
            .filter(|e| e.created_at >= today_start && e.status == "CAPTURED")
            .map(|e| e.charge_units)
            .sum();
        let month_usage = rows
            .iter()
            .filter(|e| e.created_at >= month_start && e.status == "CAPTURED")
            .map(|e| e.units_billed)
            .sum();
        let month_spend = rows
            .iter()
            .filter(|e| e.created_at >= month_start && e.status == "CAPTURED")
            .map(|e| e.charge_units)
            .sum();
        let mut daily_map = std::collections::BTreeMap::<String, (i64, i64)>::new();
        let mut op_map = std::collections::BTreeMap::<String, (i64, i64)>::new();
        for e in &rows {
            if e.status == "CAPTURED" {
                let d = e.created_at.date_naive().to_string();
                let x = daily_map.entry(d).or_default();
                x.0 += e.units_billed;
                x.1 += e.charge_units;
                let x = op_map.entry(e.operation.clone()).or_default();
                x.0 += e.units_billed;
                x.1 += e.charge_units;
            }
        }
        let daily = daily_map
            .into_iter()
            .map(|(day, (u, s))| UsageDailyPoint {
                day,
                usage_units: u,
                spend_units: s,
            })
            .collect();
        let by_operation = op_map
            .into_iter()
            .map(|(operation, (u, s))| UsageOperationPoint {
                operation,
                usage_units: u,
                spend_units: s,
            })
            .collect();
        let mut rows = rows;
        rows.sort_by_key(|e| std::cmp::Reverse(e.created_at));
        rows.truncate(limit.clamp(1, 500) as usize);
        Ok(UsageReport {
            today_usage_units: today_usage,
            month_usage_units: month_usage,
            month_spend_units: month_spend,
            daily,
            by_operation,
            rows,
        })
    }
}
