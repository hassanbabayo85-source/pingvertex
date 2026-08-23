use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::{CheckoutStatus, PlanTier};
use crate::shared::ids::{CheckoutSessionId, TenantId};

/// A provider-agnostic checkout session.
///
/// The domain knows only that a tenant is attempting to pay for a given
/// plan tier and whether that attempt is `Pending`, `Completed`, or
/// `Failed`. Provider-specific references live behind the
/// `PaymentProvider` port as an opaque `provider_checkout_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutSession {
    pub id: CheckoutSessionId,
    pub tenant_id: TenantId,
    pub plan_tier: PlanTier,
    pub status: CheckoutStatus,
    /// Opaque checkout identifier returned by the payment provider.
    pub provider: String,
    pub provider_checkout_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CheckoutSession {
    pub fn new_pending(
        id: CheckoutSessionId,
        tenant_id: TenantId,
        plan_tier: PlanTier,
        provider: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            plan_tier,
            status: CheckoutStatus::Pending,
            provider,
            provider_checkout_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn attach_provider_checkout_id(&mut self, provider_checkout_id: String) {
        self.provider_checkout_id = Some(provider_checkout_id);
        self.updated_at = Utc::now();
    }

    pub fn mark_completed(&mut self) {
        self.status = CheckoutStatus::Completed;
        self.updated_at = Utc::now();
    }

    pub fn mark_failed(&mut self) {
        self.status = CheckoutStatus::Failed;
        self.updated_at = Utc::now();
    }
}
