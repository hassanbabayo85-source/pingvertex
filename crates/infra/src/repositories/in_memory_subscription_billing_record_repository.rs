use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use application::ports::subscription_billing_record_repository::{
    BillingRecordStatus, InsertBillingRecordOutcome, RepoResult, SubscriptionBillingRecord,
    SubscriptionBillingRecordRepository,
};
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct InMemorySubscriptionBillingRecordRepository {
    records: Arc<RwLock<HashMap<(String, String), SubscriptionBillingRecord>>>,
}

impl InMemorySubscriptionBillingRecordRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SubscriptionBillingRecordRepository for InMemorySubscriptionBillingRecordRepository {
    async fn try_insert(
        &self,
        record: &SubscriptionBillingRecord,
    ) -> RepoResult<InsertBillingRecordOutcome> {
        let key = (record.provider.clone(), record.provider_reference.clone());
        let mut map = self.records.write().unwrap();
        if map.contains_key(&key) {
            Ok(InsertBillingRecordOutcome::AlreadyExists)
        } else {
            map.insert(key, record.clone());
            Ok(InsertBillingRecordOutcome::Inserted)
        }
    }

    async fn mark_succeeded(
        &self,
        provider: &str,
        provider_reference: &str,
        provider_payment_id: &str,
    ) -> RepoResult<bool> {
        let key = (provider.to_owned(), provider_reference.to_owned());
        let mut map = self.records.write().unwrap();
        if let Some(record) = map.get_mut(&key) {
            record.status = BillingRecordStatus::Succeeded;
            record.provider_payment_id = Some(provider_payment_id.to_owned());
            record.updated_at = chrono::Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn find_by_provider_reference(
        &self,
        provider: &str,
        provider_reference: &str,
    ) -> RepoResult<Option<SubscriptionBillingRecord>> {
        let key = (provider.to_owned(), provider_reference.to_owned());
        Ok(self.records.read().unwrap().get(&key).cloned())
    }
}

impl InMemorySubscriptionBillingRecordRepository {
    /// Test-support helper used by `InMemorySubscriptionActivationRepository`.
    ///
    /// The `SubscriptionBillingRecordRepository` trait deliberately does NOT
    /// expose a way to mutate `subscription_id` on an existing record, because
    /// in production the winner-linkage update (Step 4b of the F5 design) runs
    /// inside `PostgresSubscriptionActivationRepository`'s SQL transaction.
    /// The in-memory activation adapter cannot open a transaction, so it needs
    /// a direct mutator. This method exists ONLY for that purpose and MUST NOT
    /// be called from production code.
    ///
    /// Returns `true` if the record existed and was updated, `false` otherwise.
    pub fn link_subscription_id(
        &self,
        provider: &str,
        provider_reference: &str,
        subscription_id: uuid::Uuid,
    ) -> bool {
        let key = (provider.to_owned(), provider_reference.to_owned());
        let mut map = self.records.write().unwrap();
        if let Some(record) = map.get_mut(&key) {
            record.subscription_id = subscription_id;
            record.updated_at = chrono::Utc::now();
            true
        } else {
            false
        }
    }
}
