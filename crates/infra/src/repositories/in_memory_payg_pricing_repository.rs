use application::ports::{
    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{PAYGPricing, PaygOperation};
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct InMemoryPAYGPricingRepository {
    rows: Arc<RwLock<Vec<PAYGPricing>>>,
}
impl InMemoryPAYGPricingRepository {
    pub fn new(rows: Vec<PAYGPricing>) -> Self {
        Self {
            rows: Arc::new(RwLock::new(rows)),
        }
    }
    pub fn insert(&self, row: PAYGPricing) {
        self.rows.write().unwrap().push(row);
    }
}
#[async_trait]
impl PAYGPricingRepository for InMemoryPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let now = chrono::Utc::now();
        let row = self
            .rows
            .read()
            .unwrap()
            .iter()
            .filter(|p| p.operation == operation && p.active && p.effective_from <= now)
            .filter(|p| p.effective_until.map(|v| v > now).unwrap_or(true))
            .max_by_key(|p| p.version)
            .cloned();
        Ok(row)
    }
    async fn find_active_for_context(
        &self,
        operation: &str,
        _context: &PricingContext,
    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        // The in-memory adapter has no contextual columns; preserve the same
        // safe generic fallback used by production when no override exists.
        self.find_active(operation).await
    }
    async fn create_version_with_context(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
        _context: &PricingRuleContext,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        self.create_version(operation, unit_name, unit_price, currency, effective_from)
            .await
    }
    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let now = chrono::Utc::now();
        let rows = self.rows.read().unwrap();
        let mut latest = std::collections::BTreeMap::<String, PAYGPricing>::new();
        for price in rows.iter().filter(|p| {
            p.active
                && PaygOperation::is_canonical(&p.operation)
                && p.effective_from <= now
                && p.effective_until.map(|v| v > now).unwrap_or(true)
        }) {
            latest
                .entry(price.operation.clone())
                .and_modify(|current| {
                    if price.version > current.version {
                        *current = price.clone();
                    }
                })
                .or_insert_with(|| price.clone());
        }
        Ok(latest.into_values().collect())
    }
    async fn create_version(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        if !PaygOperation::is_canonical(operation) {
            return Err("unsupported PAYG operation".into());
        }
        if unit_price <= 0 || currency != "USD" {
            return Err("invalid PAYG price".into());
        }
        let mut rows = self.rows.write().unwrap();
        let version = rows.iter().map(|p| p.version).max().unwrap_or(0) + 1;
        let row = PAYGPricing {
            id: uuid::Uuid::new_v4(),
            version,
            operation: operation.to_owned(),
            unit_name: unit_name.to_owned(),
            unit_price,
            currency: currency.to_owned(),
            effective_from,
            effective_until: None,
            active: true,
        };
        rows.push(row.clone());
        Ok(row)
    }
}
