use std::error::Error;
use std::sync::{Arc, Mutex};

use application::ports::SubscriptionUsagePeriodRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::SubscriptionUsagePeriod;
use domain::shared::ids::{SubscriptionId, SubscriptionUsagePeriodId};

/// Test-only in-memory stand-in for `SubscriptionUsagePeriodRepository`.
///
/// Mirrors `InMemorySubscriptionRepository`'s style: a plain
/// `Mutex<Vec<..>>` behind an `Arc` so cloned handles share state, with
/// no persistence beyond the life of the test.
#[derive(Debug, Clone, Default)]
pub struct InMemorySubscriptionUsagePeriodRepository {
    periods: Arc<Mutex<Vec<SubscriptionUsagePeriod>>>,
}

impl InMemorySubscriptionUsagePeriodRepository {
    pub fn new() -> Self {
        Self {
            periods: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl SubscriptionUsagePeriodRepository for InMemorySubscriptionUsagePeriodRepository {
    async fn create_period(
        &self,
        period: &SubscriptionUsagePeriod,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.periods.lock().unwrap().push(period.clone());
        Ok(())
    }

    async fn close_period(
        &self,
        period_id: SubscriptionUsagePeriodId,
        end_at: DateTime<Utc>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut periods = self.periods.lock().unwrap();
        if let Some(period) = periods.iter_mut().find(|p| p.id == period_id) {
            period.end_at = Some(end_at);
        }
        Ok(())
    }

    async fn find_open_period(
        &self,
        subscription_id: SubscriptionId,
    ) -> Result<Option<SubscriptionUsagePeriod>, Box<dyn Error + Send + Sync>> {
        Ok(self
            .periods
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.subscription_id == subscription_id && p.end_at.is_none())
            .cloned())
    }

    async fn list_periods_for_cycle(
        &self,
        subscription_id: SubscriptionId,
        cycle_start: DateTime<Utc>,
        cycle_end: DateTime<Utc>,
    ) -> Result<Vec<SubscriptionUsagePeriod>, Box<dyn Error + Send + Sync>> {
        let mut result: Vec<SubscriptionUsagePeriod> = self
            .periods
            .lock()
            .unwrap()
            .iter()
            .filter(|p| {
                p.subscription_id == subscription_id
                    && p.start_at < cycle_end
                    && p.end_at.map(|e| e > cycle_start).unwrap_or(true)
            })
            .cloned()
            .collect();
        result.sort_by_key(|p| p.start_at);
        Ok(result)
    }
}
