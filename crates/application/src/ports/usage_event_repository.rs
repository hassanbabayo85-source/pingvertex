use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::UsageEvent;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type UsageEventRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct UsageReport {
    pub today_usage_units: i64,
    pub month_usage_units: i64,
    pub month_spend_units: i64,
    pub daily: Vec<UsageDailyPoint>,
    pub by_operation: Vec<UsageOperationPoint>,
    pub rows: Vec<UsageEvent>,
}
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct UsageDailyPoint {
    pub day: String,
    pub usage_units: i64,
    pub spend_units: i64,
}
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
pub struct UsageOperationPoint {
    pub operation: String,
    pub usage_units: i64,
    pub spend_units: i64,
}

#[async_trait]
pub trait UsageEventRepository: Send + Sync {
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>>;

    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent>;
    async fn report(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        operation: Option<&str>,
        status: Option<&str>,
        limit: u32,
    ) -> UsageEventRepoResult<UsageReport>;
}

#[async_trait]
impl<T> UsageEventRepository for Arc<T>
where
    T: UsageEventRepository + Send + Sync + ?Sized,
{
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>> {
        (**self).find_by_request(tenant_id, request_id).await
    }

    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        (**self).create(event).await
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
        (**self)
            .report(tenant_id, from, to, operation, status, limit)
            .await
    }
}
