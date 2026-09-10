use application::ports::{
    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PAYGPricing;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::sync::RwLock;
use uuid::Uuid;

struct RuleRecord {
    pricing: PAYGPricing,
    context: PricingRuleContext,
}

pub struct InMemoryPAYGPricingRepository {
    rows: RwLock<Vec<RuleRecord>>,
    next_version: AtomicI64,
}

impl InMemoryPAYGPricingRepository {
    pub fn new() -> Self {
        Self {
            rows: RwLock::new(Vec::new()),
            next_version: AtomicI64::new(1),
        }
    }
}

impl Default for InMemoryPAYGPricingRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PAYGPricingRepository for InMemoryPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let guard = self.rows.read().await;
        // Generic fallback logic
        let mut generic_match = None;
        for record in guard.iter() {
            if record.pricing.operation == operation
                && record.pricing.active
                && record.pricing.effective_until.is_none()
                && record.context.account_type.is_none()
                && record.context.region.is_none()
                && record.context.monitor_kind.is_none()
                && record.context.min_quantity.is_none()
                && record.context.max_quantity.is_none()
            {
                generic_match = Some(record.pricing.clone());
            }
        }
        Ok(generic_match)
    }

    async fn find_active_for_context(
        &self,
        operation: &str,
        context: &PricingContext,
    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let guard = self.rows.read().await;
        let mut candidates = Vec::new();

        for record in guard.iter() {
            if record.pricing.operation != operation
                || !record.pricing.active
                || record.pricing.effective_until.is_some()
            {
                continue;
            }

            // Check dimensions. A rule must either have no requirement (None) or match exactly.
            if let Some(ref acc_type) = record.context.account_type {
                if Some(acc_type) != context.account_type.as_ref() {
                    continue;
                }
            }
            if let Some(ref reg) = record.context.region {
                if Some(reg) != context.region.as_ref() {
                    continue;
                }
            }
            if let Some(ref mk) = record.context.monitor_kind {
                if Some(mk) != context.monitor_kind.as_ref() {
                    continue;
                }
            }
            if let Some(min_q) = record.context.min_quantity {
                if context.quantity < min_q {
                    continue;
                }
            }
            if let Some(max_q) = record.context.max_quantity {
                if context.quantity > max_q {
                    continue;
                }
            }

            // Calculate "specificity" score (higher is more specific)
            let mut score = 0;
            if record.context.account_type.is_some() {
                score += 1;
            }
            if record.context.region.is_some() {
                score += 1;
            }
            if record.context.monitor_kind.is_some() {
                score += 1;
            }
            if record.context.min_quantity.is_some() || record.context.max_quantity.is_some() {
                score += 1;
            }

            candidates.push((score, record.pricing.clone()));
        }

        if candidates.is_empty() {
            return Ok(None);
        }

        candidates.sort_by(|a, b| b.0.cmp(&a.0)); // Descending by score
        let top_score = candidates[0].0;

        let top_candidates: Vec<_> = candidates
            .into_iter()
            .filter(|c| c.0 == top_score)
            .collect();

        if top_candidates.len() > 1 {
            return Err("ambiguous pricing rules match the context with equal specificity".into());
        }

        Ok(Some(top_candidates[0].1.clone()))
    }

    async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
        let guard = self.rows.read().await;
        Ok(guard
            .iter()
            .filter(|r| r.pricing.active && r.pricing.effective_until.is_none())
            .map(|r| r.pricing.clone())
            .collect())
    }

    async fn create_version(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        self.create_version_with_context(
            operation,
            unit_name,
            unit_price,
            currency,
            effective_from,
            &PricingRuleContext::default(),
        )
        .await
    }

    async fn create_version_with_context(
        &self,
        operation: &str,
        unit_name: &str,
        unit_price: i64,
        currency: &str,
        effective_from: DateTime<Utc>,
        context: &PricingRuleContext,
    ) -> PaygPricingRepoResult<PAYGPricing> {
        let mut guard = self.rows.write().await;

        // Deactivate old rule with EXACT same context
        for record in guard.iter_mut() {
            if record.pricing.operation == operation
                && record.pricing.effective_until.is_none()
                && record.context.account_type == context.account_type
                && record.context.region == context.region
                && record.context.monitor_kind == context.monitor_kind
                && record.context.min_quantity == context.min_quantity
                && record.context.max_quantity == context.max_quantity
            {
                record.pricing.effective_until = Some(effective_from);
            }
        }

        let version = self.next_version.fetch_add(1, Ordering::SeqCst);
        let pricing = PAYGPricing {
            id: Uuid::new_v4(),
            version,
            operation: operation.to_owned(),
            unit_name: unit_name.to_owned(),
            unit_price,
            currency: currency.to_owned(),
            effective_from,
            effective_until: None,
            active: true,
        };

        guard.push(RuleRecord {
            pricing: pricing.clone(),
            context: context.clone(),
        });

        Ok(pricing)
    }
}
