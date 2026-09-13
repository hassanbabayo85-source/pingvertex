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
                "https://checkout.fake.invalid/{checkout_id}?tier={}&amount={}&currency={}",
                request.plan_tier.as_ref().map(|t| t.as_str()).unwrap_or("addon"),
                request.amount_minor,
                request.currency
            ),
        })
    }

    async fn verify_payment(
        &self,
        _provider_checkout_id: &str,
    ) -> PaymentProviderResult<PaymentVerification> {
        Ok(PaymentVerification::Paid {
            provider_payment_id: format!("fake-payment-{}", _provider_checkout_id),
            provider_reference: _provider_checkout_id.to_owned(),
            amount_units: 1_000_000,
            currency: "USD".to_owned(),
        })
    }
    async fn create_wallet_top_up(
        &self,
        request: &application::ports::WalletTopUpRequest,
    ) -> PaymentProviderResult<CheckoutDetails> {
        let id = Uuid::new_v4().to_string();
        Ok(CheckoutDetails {
            provider_checkout_id: id.clone(),
            checkout_url: format!(
                "https://checkout.fake.invalid/{id}?type=wallet_top_up&amount={}&currency={}",
                request.amount_units, request.currency
            ),
        })
    }
    async fn charge_saved_payment_method(
        &self,
        request: &application::ports::WalletTopUpRequest,
    ) -> PaymentProviderResult<PaymentVerification> {
        if request.payment_method.as_deref().unwrap_or("").is_empty() {
            return Ok(PaymentVerification::Failed {
                provider_payment_id: None,
                provider_reference: None,
                amount_units: None,
                currency: None,
            });
        }
        Ok(PaymentVerification::Paid {
            provider_payment_id: format!("fake-payment-{}", request.idempotency_key),
            provider_reference: request.idempotency_key.clone(),
            amount_units: request.amount_units,
            currency: request.currency.clone(),
        })
    }
    async fn refund_payment(
        &self,
        _request: &application::ports::RefundRequest,
    ) -> PaymentProviderResult<application::ports::RefundDetails> {
        Ok(application::ports::RefundDetails {
            provider_refund_id: Uuid::new_v4().to_string(),
        })
    }
}
