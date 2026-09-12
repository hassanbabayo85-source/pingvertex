use application::ports::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
};
use async_trait::async_trait;

/// Billing provider used when `BILLING_PROVIDER=disabled` (or is unset).
///
/// Every operation fails closed. No checkout can be initiated and no
/// payment can be verified, which is exactly what a deployment without a
/// configured billing provider should do.
#[derive(Debug, Clone, Default)]
pub struct DisabledBillingProvider;

#[async_trait]
impl PaymentProvider for DisabledBillingProvider {
    fn provider_name(&self) -> &'static str {
        "disabled"
    }

    async fn create_checkout(
        &self,
        _request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }

    async fn verify_payment(
        &self,
        _provider_checkout_id: &str,
    ) -> PaymentProviderResult<PaymentVerification> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }
}
