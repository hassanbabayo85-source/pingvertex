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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentVerification {
    Paid {
        provider_payment_id: String,
        provider_reference: String,
        amount_units: i64,
        currency: String,
    },
    Pending {
        provider_payment_id: Option<String>,
        provider_reference: Option<String>,
        amount_units: Option<i64>,
        currency: Option<String>,
    },
    Failed {
        provider_payment_id: Option<String>,
        provider_reference: Option<String>,
        amount_units: Option<i64>,
        currency: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutRequest {
    pub tenant_id: TenantId,
    /// The subscription tier this checkout is for, when it is a
    /// subscription checkout. `None` for add-on purchases.
    pub plan_tier: Option<PlanTier>,
    pub amount_minor: i64,
    pub currency: String,
    pub customer_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletTopUpRequest {
    pub tenant_id: TenantId,
    pub amount_units: i64,
    pub currency: String,
    /// Provider-side idempotency key. Must be unique per payment attempt.
    pub idempotency_key: String,
    pub payment_method: Option<String>,
    /// Customer email paired with the reusable provider authorization.
    pub customer_email: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundRequest {
    pub tenant_id: TenantId,
    pub provider_payment_id: String,
    pub amount_units: i64,
    pub currency: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundDetails {
    pub provider_refund_id: String,
}

#[derive(Debug)]
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

    async fn create_wallet_top_up(
        &self,
        _request: &WalletTopUpRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("wallet top-up is not supported by this billing provider").into())
    }

    async fn charge_saved_payment_method(
        &self,
        _request: &WalletTopUpRequest,
    ) -> PaymentProviderResult<PaymentVerification> {
        Err(std::io::Error::other(
            "automatic wallet top-up is not supported by this billing provider",
        )
        .into())
    }

    async fn refund_payment(
        &self,
        _request: &RefundRequest,
    ) -> PaymentProviderResult<RefundDetails> {
        Err(std::io::Error::other("refunds are not supported by this billing provider").into())
    }
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
    async fn create_wallet_top_up(
        &self,
        request: &WalletTopUpRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        (**self).create_wallet_top_up(request).await
    }
    async fn charge_saved_payment_method(
        &self,
        request: &WalletTopUpRequest,
    ) -> PaymentProviderResult<PaymentVerification> {
        (**self).charge_saved_payment_method(request).await
    }
    async fn refund_payment(
        &self,
        request: &RefundRequest,
    ) -> PaymentProviderResult<RefundDetails> {
        (**self).refund_payment(request).await
    }
}
