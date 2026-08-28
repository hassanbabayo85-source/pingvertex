use async_trait::async_trait;
use rust_decimal::Decimal;
use std::{error::Error, sync::Arc};
pub type SmsPricingResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[async_trait]
pub trait SmsProviderPricing: Send + Sync {
    async fn outbound_sms_unit_price(
        &self,
        iso_country: &str,
    ) -> SmsPricingResult<(Decimal, String)>;
}
#[async_trait]
impl<T> SmsProviderPricing for Arc<T>
where
    T: SmsProviderPricing + Send + Sync + ?Sized,
{
    async fn outbound_sms_unit_price(&self, c: &str) -> SmsPricingResult<(Decimal, String)> {
        (**self).outbound_sms_unit_price(c).await
    }
}
