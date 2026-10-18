//! Persistence for `subscription_billing_records` — the append-only
//! audit + idempotency log for subscription checkout activations.
//!
//! The unique constraint on `(provider, provider_reference)` is the
//! activation idempotency mechanism: `try_insert` returns `Inserted`
//! exactly once per provider reference. Concurrent verifies race on
//! the INSERT; only one wins.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use domain::PlanTier;
use domain::money::{BillingInterval, Currency};

pub type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingRecordStatus {
    Pending,
    Succeeded,
    Failed,
}

impl BillingRecordStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for BillingRecordStatus {
    type Err = Box<dyn std::error::Error + Send + Sync>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => Err(std::io::Error::other(format!(
                "invalid billing record status: {other}"
            ))
            .into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionBillingRecord {
    pub id: uuid::Uuid,
    pub subscription_id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub plan_tier: PlanTier,
    pub billing_interval: BillingInterval,
    pub currency: Currency,
    pub amount_minor: i64,
    pub status: BillingRecordStatus,
    pub provider: String,
    pub provider_reference: String,
    pub provider_payment_id: Option<String>,
    pub provider_subscription_code: Option<String>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertBillingRecordOutcome {
    Inserted,
    AlreadyExists,
}

#[async_trait]
pub trait SubscriptionBillingRecordRepository: Send + Sync {
    /// Idempotent insert. Returns `Inserted` for the first record for
    /// `(provider, provider_reference)`; `AlreadyExists` otherwise.
    async fn try_insert(
        &self,
        record: &SubscriptionBillingRecord,
    ) -> RepoResult<InsertBillingRecordOutcome>;

    async fn mark_succeeded(
        &self,
        provider: &str,
        provider_reference: &str,
        provider_payment_id: &str,
    ) -> RepoResult<bool>;

    async fn find_by_provider_reference(
        &self,
        provider: &str,
        provider_reference: &str,
    ) -> RepoResult<Option<SubscriptionBillingRecord>>;
}

#[async_trait]
impl<T> SubscriptionBillingRecordRepository for std::sync::Arc<T>
where
    T: SubscriptionBillingRecordRepository + ?Sized,
{
    async fn try_insert(
        &self,
        record: &SubscriptionBillingRecord,
    ) -> RepoResult<InsertBillingRecordOutcome> {
        (**self).try_insert(record).await
    }

    async fn mark_succeeded(
        &self,
        provider: &str,
        provider_reference: &str,
        provider_payment_id: &str,
    ) -> RepoResult<bool> {
        (**self)
            .mark_succeeded(provider, provider_reference, provider_payment_id)
            .await
    }

    async fn find_by_provider_reference(
        &self,
        provider: &str,
        provider_reference: &str,
    ) -> RepoResult<Option<SubscriptionBillingRecord>> {
        (**self)
            .find_by_provider_reference(provider, provider_reference)
            .await
    }
}
