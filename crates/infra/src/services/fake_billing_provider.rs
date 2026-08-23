use application::ports::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
};
use async_trait::async_trait;
use uuid::Uuid;

/// A development/testing billing provider.
///
/// `create_checkout` always succeeds; `verify_payment` always returns
/// `Paid`. This is intentionally simplistic: production deployments must
/// not use this provider. It exists so B14's HTTP and application layers
/// can be exercised without any third-party account or secrets.
#[derive(Debug, Clone, Default)]
pub struct FakeBillingProvider;

#[async_trait]
impl PaymentProvider for FakeBillingProvider {
    fn provider_name(&self) -> &'static str {
        "fake"
    }

    async fn create_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        let checkout_id = Uuid::new_v4().to_string();
        Ok(CheckoutDetails {
            provider_checkout_id: checkout_id.clone(),
            checkout_url: format!(
                "https://checkout.fake.invalid/{checkout_id}?tier={}",
                request.plan_tier.as_str()
            ),
        })
    }

    async fn verify_payment(
        &self,
        _provider_checkout_id: &str,
    ) -> PaymentProviderResult<PaymentVerification> {
        Ok(PaymentVerification::Paid)
    }
}
