use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::{CheckoutStatus, PlanTier};
use crate::money::{BillingInterval, Currency};
use crate::shared::ids::{CheckoutSessionId, TenantId};
use rust_decimal::Decimal;
use uuid::Uuid;

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
    /// The subscription tier this session is for, when the session is
    /// a subscription checkout. `None` for add-on purchases (SMS
    /// credits, Enterprise SLA, extended retention), which have no plan
    /// tier.
    pub plan_tier: Option<PlanTier>,
    pub status: CheckoutStatus,
    /// Opaque checkout identifier returned by the payment provider.
    pub provider: String,
    pub provider_checkout_id: Option<String>,
    pub plan_change_id: Option<Uuid>,
    pub addon_subscription_id: Option<Uuid>,
    pub amount: Option<Decimal>,
    pub currency: Option<String>,
    /// Task 6: billing interval for subscription checkout sessions.
    /// `None` for legacy plan-change / add-on sessions.
    pub billing_interval: Option<BillingInterval>,
    /// Task 6: server-quoted integer minor units (kobo / cents).
    /// Distinct from `amount` (Decimal, major units) which the
    /// plan-change and add-on paths continue to use.
    pub amount_minor: Option<i64>,
    /// Task 6: provider transaction id, set after successful verify.
    pub provider_payment_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CheckoutSession {
    pub fn new_pending(
        id: CheckoutSessionId,
        tenant_id: TenantId,
        plan_tier: Option<PlanTier>,
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
            plan_change_id: None,
            addon_subscription_id: None,
            amount: None,
            currency: None,
            billing_interval: None,
            amount_minor: None,
            provider_payment_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn attach_provider_checkout_id(&mut self, provider_checkout_id: String) {
        self.provider_checkout_id = Some(provider_checkout_id);
        self.updated_at = Utc::now();
    }

    /// Task 6: attaches the server-quoted subscription price. The
    /// amount is captured at initiate time and validated against the
    /// provider's reported amount at verify time, so a price-version
    /// bump between the two steps does not silently change what the
    /// customer pays.
    pub fn attach_subscription_billing(
        &mut self,
        billing_interval: BillingInterval,
        amount_minor: i64,
        currency: Currency,
    ) {
        self.billing_interval = Some(billing_interval);
        self.amount_minor = Some(amount_minor);
        self.currency = Some(currency.as_str().to_owned());
        self.updated_at = Utc::now();
    }

    /// Task 6: records the provider's transaction id after verification.
    pub fn attach_provider_payment_id(&mut self, provider_payment_id: String) {
        self.provider_payment_id = Some(provider_payment_id);
        self.updated_at = Utc::now();
    }

    pub fn attach_plan_change(&mut self, id: Uuid, amount: Decimal, currency: String) {
        self.plan_change_id = Some(id);
        self.amount = Some(amount);
        self.currency = Some(currency);
        self.updated_at = Utc::now()
    }
    pub fn attach_addon(&mut self, id: Uuid, amount: Decimal, currency: String) {
        self.addon_subscription_id = Some(id);
        self.amount = Some(amount);
        self.currency = Some(currency);
        self.updated_at = Utc::now()
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
