use async_trait::async_trait;
use std::{error::Error, sync::Arc};

use domain::enums::PlanTier;
use domain::shared::ids::TenantId;

pub type PaymentProviderResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Server-side payment verification result.
///
/// IMPORTANT: this is ALWAYS produced by calling the provider. Clients
/// never report this value; a client only tells us which checkout
/// session it wants to verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentVerification {
    Paid,
    Pending,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckoutRequest {
    pub tenant_id: TenantId,
    pub plan_tier: PlanTier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutDetails {
    pub provider_checkout_id: String,
    pub checkout_url: String,
}

/// Provider-agnostic billing port.
///
/// Concrete implementations live in `infra` and are selected by
/// `BILLING_PROVIDER`/`BILLING_SECRET_KEY` config at boot time. No
/// provider-specific types or behavior leak into domain/application.
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails>;

    async fn verify_payment(
        &self,
        provider_checkout_id: &str,
    ) -> PaymentProviderResult<PaymentVerification>;
}

#[async_trait]
impl<T> PaymentProvider for Arc<T>
where
    T: PaymentProvider + Send + Sync + ?Sized,
{
    fn provider_name(&self) -> &'static str {
        (**self).provider_name()
    }

    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        (**self).create_checkout(request).await
    }

    async fn verify_payment(
        &self,
        provider_checkout_id: &str,
    ) -> PaymentProviderResult<PaymentVerification> {
        (**self).verify_payment(provider_checkout_id).await
    }
}
