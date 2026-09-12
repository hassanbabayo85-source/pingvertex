use async_trait::async_trait;
use domain::Payment;
use std::{error::Error, sync::Arc};
pub type PaymentRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[async_trait]
pub trait PaymentRepository: Send + Sync {
    async fn record(&self, payment: &Payment) -> PaymentRepoResult<()>;
    async fn find_by_provider_payment_id(
        &self,
        provider: &str,
        provider_payment_id: &str,
    ) -> PaymentRepoResult<Option<Payment>>;
}
#[async_trait]
impl<T> PaymentRepository for Arc<T>
where
    T: PaymentRepository + Send + Sync + ?Sized,
{
    async fn record(&self, p: &Payment) -> PaymentRepoResult<()> {
        (**self).record(p).await
    }
    async fn find_by_provider_payment_id(
        &self,
        a: &str,
        b: &str,
    ) -> PaymentRepoResult<Option<Payment>> {
        (**self).find_by_provider_payment_id(a, b).await
    }
}
