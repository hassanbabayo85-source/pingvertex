use super::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase,
    MeteredAttempt,
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
    /// Trusted runtime monitor context.
    pub monitor_id: Option<Uuid>,
    /// Trusted durable execution context.
    pub execution_id: Option<Uuid>,
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

    pub fn with_audit_log_repository(
        mut self,
        repo: Arc<dyn crate::ports::FinancialAuditLogRepository>,
    ) -> Self {
        self.charge = self.charge.with_audit_log_repository(repo);
        self
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
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                },
                operation,
            )
            .await
    }

    pub async fn find_by_request(
        &self,
        tenant_id: uuid::Uuid,
        request_id: &str,
    ) -> Result<Option<domain::UsageEvent>, ChargeForUsageError> {
        self.charge.find_usage_event(tenant_id, request_id).await
    }

    /// Monitoring checks are billable attempts even when the target fails.
    pub async fn execute_attempt<F, Fut, E>(
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
            .execute_attempt(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                },
                operation,
            )
            .await
    }

    /// Runtime boundary with explicit attempted/not-attempted classification.
    pub async fn execute_classified_attempt<F, Fut, E>(
        &self,
        input: MeterUsageInput,
        operation: F,
    ) -> Result<(ChargeForUsageResult, MeteredAttempt), ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<MeteredAttempt, E>>,
        E: fmt::Display,
    {
        self.charge
            .execute_classified_attempt(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                },
                operation,
            )
            .await
    }

    pub async fn execute_classified_attempt_with_output<F, Fut, E, T>(
        &self,
        input: MeterUsageInput,
        operation: F,
    ) -> Result<(ChargeForUsageResult, Option<T>), ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(MeteredAttempt, T), E>>,
        E: fmt::Display,
    {
        self.charge
            .execute_classified_attempt_with_output(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                },
                operation,
            )
            .await
    }

    /// Finalizes a reservation created before a remote agent started its
    /// outbound operation. No external operation is executed here.
    pub async fn finalize_reserved_attempt(
        &self,
        input: MeterUsageInput,
        attempted: bool,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError> {
        self.charge
            .finalize_reserved_attempt(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                },
                attempted,
            )
            .await
    }

    /// Durable-worker variant: permits resuming a PENDING reservation after a
    /// scheduler lease recovery while preserving wallet idempotency.
    pub async fn execute_recoverable<F, Fut, E>(
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
            .execute_recoverable(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                },
                operation,
            )
            .await
    }
}
