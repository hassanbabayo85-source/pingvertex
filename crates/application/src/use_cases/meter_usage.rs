use super::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase,
};
use crate::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterUsageInput {
    pub tenant_id: Uuid,
    pub operation: String,
    pub units: i64,
    pub request_id: String,
}

pub struct MeterUsageUseCase {
    charge: ChargeForUsageUseCase,
}
impl MeterUsageUseCase {
    pub fn new(
        wallet_repository: Arc<dyn WalletRepository>,
        pricing_repository: Arc<dyn PAYGPricingRepository>,
        usage_event_repository: Arc<dyn UsageEventRepository>,
    ) -> Self {
        Self {
            charge: ChargeForUsageUseCase::new(
                wallet_repository,
                pricing_repository,
                usage_event_repository,
            ),
        }
    }

    /// Authentication/tenant identification belongs to the API/auth boundary;
    /// this use case accepts only the authenticated tenant id and never trusts
    /// a client-supplied price or total.
    pub async fn execute<F, Fut, E>(
        &self,
        input: MeterUsageInput,
        operation: F,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        self.charge
            .execute(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                },
                operation,
            )
            .await
    }
}
