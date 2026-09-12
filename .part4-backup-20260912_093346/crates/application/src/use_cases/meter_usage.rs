use super::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase,
    MeteredAttempt,
};
use crate::ports::{
    PAYGPricingRepository, PricingContext, TenantRepository, UsageEventRepository, WalletRepository,
};
use std::collections::HashMap;
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
    /// Trusted runtime pricing dimensions; public API inputs must leave this None.
    pub pricing_context: Option<crate::ports::PricingContext>,
}

pub struct MeterUsageUseCase {
    charge: ChargeForUsageUseCase,
    tenant_repository: Option<Arc<dyn TenantRepository>>,
    account_type_cache: Arc<std::sync::RwLock<HashMap<Uuid, domain::AccountType>>>,
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
            tenant_repository: None,
            account_type_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn with_tenant_repository(mut self, repository: Arc<dyn TenantRepository>) -> Self {
        self.tenant_repository = Some(repository);
        self
    }

    async fn enrich_pricing_context(
        &self,
        tenant_id: Uuid,
        context: Option<PricingContext>,
        quantity: u64,
    ) -> Option<PricingContext> {
        let mut context = context.unwrap_or_default();
        context.quantity = quantity;
        if context.account_type.is_none() {
            if let Some(account_type) = self
                .account_type_cache
                .read()
                .ok()
                .and_then(|cache| cache.get(&tenant_id).copied())
            {
                context.account_type = Some(account_type);
            } else if let Some(repository) = &self.tenant_repository
                && let Ok(Some(tenant)) = repository.find_by_id(domain::TenantId(tenant_id)).await
            {
                context.account_type = Some(tenant.account_type);
                if let Ok(mut cache) = self.account_type_cache.write() {
                    cache.insert(tenant_id, tenant.account_type);
                }
            }
        }
        Some(context)
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
        let pricing_context = self
            .enrich_pricing_context(
                input.tenant_id,
                input.pricing_context.clone(),
                input.units.max(0) as u64,
            )
            .await;
        self.charge
            .execute(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                    pricing_context: pricing_context.clone(),
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
        let pricing_context = self
            .enrich_pricing_context(
                input.tenant_id,
                input.pricing_context.clone(),
                input.units.max(0) as u64,
            )
            .await;
        self.charge
            .execute_attempt(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                    pricing_context: pricing_context.clone(),
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
        let pricing_context = self
            .enrich_pricing_context(
                input.tenant_id,
                input.pricing_context.clone(),
                input.units.max(0) as u64,
            )
            .await;
        self.charge
            .execute_classified_attempt(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                    pricing_context: pricing_context.clone(),
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
        let pricing_context = self
            .enrich_pricing_context(
                input.tenant_id,
                input.pricing_context.clone(),
                input.units.max(0) as u64,
            )
            .await;
        self.charge
            .execute_classified_attempt_with_output(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                    pricing_context: pricing_context.clone(),
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
        let pricing_context = self
            .enrich_pricing_context(
                input.tenant_id,
                input.pricing_context.clone(),
                input.units.max(0) as u64,
            )
            .await;
        self.charge
            .finalize_reserved_attempt(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                    pricing_context: pricing_context.clone(),
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
        let pricing_context = self
            .enrich_pricing_context(
                input.tenant_id,
                input.pricing_context.clone(),
                input.units.max(0) as u64,
            )
            .await;
        self.charge
            .execute_recoverable(
                ChargeForUsageInput {
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    units: input.units,
                    request_id: input.request_id,
                    monitor_id: input.monitor_id,
                    execution_id: input.execution_id,
                    pricing_context: pricing_context.clone(),
                },
                operation,
            )
            .await
    }
}
