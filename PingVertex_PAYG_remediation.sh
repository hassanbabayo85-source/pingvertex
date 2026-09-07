#!/usr/bin/env bash
set -Eeuo pipefail

echo "Applying PingVertex PAYG remediation..."

if [[ ! -f Cargo.toml ]]; then
  echo "ERROR: run this script from the PingVertex project root (Cargo.toml not found)." >&2
  exit 1
fi

BACKUP_DIR=".payg-remediation-backup-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BACKUP_DIR"

if [[ -f "crates/application/src/use_cases/charge_for_usage.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/src/use_cases/charge_for_usage.rs")"
  cp "crates/application/src/use_cases/charge_for_usage.rs" "$BACKUP_DIR/crates/application/src/use_cases/charge_for_usage.rs"
fi

mkdir -p "$(dirname "crates/application/src/use_cases/charge_for_usage.rs")"
cat > "crates/application/src/use_cases/charge_for_usage.rs" <<'PINGVERTEX_EOF'
use crate::ports::{
    FinancialAuditLogRepository, PAYGPricingRepository, UsageEventRepository, WalletRepository,
};
use chrono::{Duration, Utc};
use domain::{CreditReservation, UsageEvent, WalletTransaction};
use std::{error::Error, fmt, future::Future, sync::Arc, time::Duration as StdDuration};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeForUsageInput {
    pub tenant_id: Uuid,
    pub operation: String,
    pub units: i64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeForUsageResult {
    pub usage_event: UsageEvent,
    pub transaction: Option<WalletTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeteredAttempt {
    pub attempted: bool,
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum ChargeForUsageError {
    InvalidInput(&'static str),
    NoPricing,
    InvalidPricing(&'static str),
    InsufficientBalance,
    OperationFailed(String),
    OperationNotAttempted(String),
    Repository(Box<dyn Error + Send + Sync>),
    ReservationInProgress,
    CaptureFailed(Box<dyn Error + Send + Sync>),
}
impl fmt::Display for ChargeForUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(m) => write!(f, "{m}"),
            Self::NoPricing => write!(f, "no active PAYG pricing found"),
            Self::InvalidPricing(m) => write!(f, "{m}"),
            Self::InsufficientBalance => write!(f, "PAYG balance is insufficient"),
            Self::OperationFailed(m) => write!(f, "metered operation failed: {m}"),
            Self::OperationNotAttempted(m) => write!(f, "metered operation was not attempted: {m}"),
            Self::Repository(_) | Self::CaptureFailed(_) => {
                write!(f, "PAYG repository operation failed")
            }
            Self::ReservationInProgress => write!(f, "PAYG request is already in progress"),
        }
    }
}
impl ChargeForUsageError {
    pub fn app_code(&self) -> Option<&'static str> {
        match self {
            Self::InsufficientBalance => Some("PAYG_INSUFFICIENT_BALANCE"),
            _ => None,
        }
    }
}

impl Error for ChargeForUsageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(e) | Self::CaptureFailed(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct ChargeForUsageUseCase {
    wallet_repository: Arc<dyn WalletRepository>,
    audit_log_repository: Option<Arc<dyn FinancialAuditLogRepository>>,
    pricing_repository: Arc<dyn PAYGPricingRepository>,
    usage_event_repository: Arc<dyn UsageEventRepository>,
}
impl ChargeForUsageUseCase {
    pub fn new(
        wallet_repository: Arc<dyn WalletRepository>,
        pricing_repository: Arc<dyn PAYGPricingRepository>,
        usage_event_repository: Arc<dyn UsageEventRepository>,
    ) -> Self {
        Self {
            wallet_repository,
            pricing_repository,
            usage_event_repository,
            audit_log_repository: None,
        }
    }

    pub fn with_audit_log_repository(mut self, repo: Arc<dyn FinancialAuditLogRepository>) -> Self {
        self.audit_log_repository = Some(repo);
        self
    }

    pub async fn find_usage_event(&self, tenant_id: Uuid, request_id: &str) -> Result<Option<UsageEvent>, ChargeForUsageError> {
        self.usage_event_repository.find_by_request(tenant_id, request_id).await.map_err(ChargeForUsageError::Repository)
    }

    pub async fn execute<F, Fut, E>(
        &self,
        input: ChargeForUsageInput,
        operation: F,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        self.execute_internal(input, operation, false).await
    }

    /// Worker-safe variant for durable operations whose scheduler may retry
    /// the same claimed execution after a lease expiry. A PENDING reservation
    /// is allowed to be resumed with the same idempotency key, so the network
    /// operation can be retried while the wallet can still be debited at most
    /// once. The public `execute` API keeps its stricter "already in progress"
    /// behavior for callers that expect duplicate requests to be suppressed.
    pub async fn execute_recoverable<F, Fut, E>(
        &self,
        input: ChargeForUsageInput,
        operation: F,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        self.execute_internal(input, operation, true).await
    }

    /// Monitoring attempts are billable even when the target fails. The
    /// external operation runs once, then financial capture records the debit,
    /// usage event, and audits as one atomic repository transaction.
    pub async fn execute_attempt<F, Fut, E>(
        &self,
        input: ChargeForUsageInput,
        operation: F,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        validate_input(&input)?;
        if let Some(event) = self
            .usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id)
            .await
            .map_err(ChargeForUsageError::Repository)?
        {
            return Ok(ChargeForUsageResult {
                usage_event: event,
                transaction: None,
            });
        }
        let pricing = self
            .pricing_repository
            .find_active(&input.operation)
            .await
            .map_err(ChargeForUsageError::Repository)?
            .ok_or(ChargeForUsageError::NoPricing)?;
        if pricing.unit_price <= 0 {
            return Err(ChargeForUsageError::InvalidPricing(
                "active PAYG unit price must be positive",
            ));
        }
        let charge_units = input
            .units
            .checked_mul(pricing.unit_price)
            .ok_or(ChargeForUsageError::InvalidPricing("PAYG charge overflow"))?;
        let wallet = self
            .wallet_repository
            .get_or_create(input.tenant_id, &pricing.currency)
            .await
            .map_err(ChargeForUsageError::Repository)?;
        if wallet.currency != pricing.currency {
            return Err(ChargeForUsageError::InvalidPricing(
                "PAYG pricing currency does not match the tenant wallet currency",
            ));
        }
        let reservation = self
            .wallet_repository
            .reserve(
                input.tenant_id,
                charge_units,
                &input.operation,
                &input.request_id,
                Utc::now() + Duration::seconds(60),
            )
            .await
            .map_err(|e| {
                if e.to_string() == "insufficient wallet balance" {
                    ChargeForUsageError::InsufficientBalance
                } else {
                    ChargeForUsageError::Repository(e)
                }
            })?;
        if !reservation.created && reservation.reservation.status != CreditReservation::PENDING {
            if reservation.reservation.status == CreditReservation::CAPTURED
                && let Some(event) = self
                    .usage_event_repository
                    .find_by_request(input.tenant_id, &input.request_id)
                    .await
                    .map_err(ChargeForUsageError::Repository)?
            {
                return Ok(ChargeForUsageResult {
                    usage_event: event,
                    transaction: None,
                });
            }
            return Err(ChargeForUsageError::ReservationInProgress);
        }
        let operation_result = operation().await;
        let event = self.build_event(
            &input,
            input.units,
            pricing.unit_price,
            charge_units,
            pricing.version,
            &pricing.currency,
            "CAPTURED",
        );
        let audits = self.build_payg_audits(&input, &event);
        let transaction = self
            .wallet_repository
            .capture(reservation.reservation.id, Some(&event), &audits)
            .await
            .map_err(ChargeForUsageError::CaptureFailed)?;
        let event = self
            .usage_event_repository
            .create(&event)
            .await
            .map_err(ChargeForUsageError::Repository)?;
        if let Err(err) = operation_result {
            return Err(ChargeForUsageError::OperationFailed(format!(
                "{err}; usage_event={}",
                event.id
            )));
        }
        Ok(ChargeForUsageResult {
            usage_event: event,
            transaction: Some(transaction),
        })
    }

    /// Runtime billing boundary. Only an operation that explicitly reports
    /// `attempted=true` is captured. Pre-attempt failures release the reservation.
    pub async fn execute_classified_attempt<F, Fut, E>(
        &self, input: ChargeForUsageInput, operation: F,
    ) -> Result<(ChargeForUsageResult, MeteredAttempt), ChargeForUsageError>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<MeteredAttempt, E>>, E: fmt::Display
    {
        self.execute_classified_attempt_with_output(input, || async {
            operation().await.map(|attempt| (attempt.clone(), attempt))
        }).await.map(|(result, attempt)| (result, attempt.expect("classified attempt output")))
    }

    pub async fn execute_classified_attempt_with_output<F, Fut, E, T>(
        &self, input: ChargeForUsageInput, operation: F,
    ) -> Result<(ChargeForUsageResult, Option<T>), ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(MeteredAttempt, T), E>>,
        E: fmt::Display,
    {
        validate_input(&input)?;
        if let Some(event) = self.usage_event_repository.find_by_request(input.tenant_id, &input.request_id).await.map_err(ChargeForUsageError::Repository)? {
            return Ok((ChargeForUsageResult { usage_event: event, transaction: None }, None));
        }
        let pricing = self.pricing_repository.find_active(&input.operation).await.map_err(ChargeForUsageError::Repository)?.ok_or(ChargeForUsageError::NoPricing)?;
        if pricing.unit_price <= 0 { return Err(ChargeForUsageError::InvalidPricing("active PAYG unit price must be positive")); }
        let charge_units = input.units.checked_mul(pricing.unit_price).ok_or(ChargeForUsageError::InvalidPricing("PAYG charge overflow"))?;
        let wallet = self.wallet_repository.get_or_create(input.tenant_id, &pricing.currency).await.map_err(ChargeForUsageError::Repository)?;
        if wallet.currency != pricing.currency { return Err(ChargeForUsageError::InvalidPricing("PAYG pricing currency does not match the tenant wallet currency")); }
        let reservation = self.wallet_repository.reserve(input.tenant_id, charge_units, &input.operation, &input.request_id, Utc::now() + Duration::seconds(60)).await.map_err(|e| {
            if e.to_string() == "insufficient wallet balance" { ChargeForUsageError::InsufficientBalance } else { ChargeForUsageError::Repository(e) }
        })?;
        if !reservation.created {
            if reservation.reservation.status == CreditReservation::CAPTURED {
                if let Some(event) = self.usage_event_repository
                    .find_by_request(input.tenant_id, &input.request_id)
                    .await
                    .map_err(ChargeForUsageError::Repository)?
                {
                    return Ok((ChargeForUsageResult { usage_event: event, transaction: None }, None));
                }
            }
            // Another live worker owns the reservation. Never run the external
            // operation twice for the same durable execution identity. If the
            // owner crashed, the reservation expiry worker will make the request
            // reusable and the scheduler can retry it.
            return Err(ChargeForUsageError::ReservationInProgress);
        }
        let (attempt, output) = match operation().await {
            Ok(value) => value,
            Err(err) => {
                self.wallet_repository.release(reservation.reservation.id).await.map_err(ChargeForUsageError::Repository)?;
                return Err(ChargeForUsageError::OperationNotAttempted(err.to_string()));
            }
        };
        if !attempt.attempted {
            self.wallet_repository.release(reservation.reservation.id).await.map_err(ChargeForUsageError::Repository)?;
            return Err(ChargeForUsageError::OperationNotAttempted(attempt.error.unwrap_or_else(|| "pre-attempt failure".to_owned())));
        }
        let event = self.build_event(&input, input.units, pricing.unit_price, charge_units, pricing.version, &pricing.currency, "CAPTURED");
        let audits = self.build_payg_audits(&input, &event);
        let transaction = self.wallet_repository.capture(reservation.reservation.id, Some(&event), &audits).await.map_err(ChargeForUsageError::CaptureFailed)?;
        let event = self.usage_event_repository.create(&event).await.map_err(ChargeForUsageError::Repository)?;
        if let Some(err) = attempt.error { return Err(ChargeForUsageError::OperationFailed(format!("{err}; usage_event={}", event.id))); }
        Ok((ChargeForUsageResult { usage_event: event, transaction: Some(transaction) }, Some(output)))
    }

    async fn execute_internal<F, Fut, E>(
        &self,
        input: ChargeForUsageInput,
        operation: F,
        retry_pending: bool,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        validate_input(&input)?;

        // Fast idempotency path: a completed request always returns its
        // original result and never calls the operation again.
        if let Some(event) = self
            .usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id)
            .await
            .map_err(ChargeForUsageError::Repository)?
        {
            return Ok(ChargeForUsageResult {
                usage_event: event,
                transaction: None,
            });
        }

        let pricing = self
            .pricing_repository
            .find_active(&input.operation)
            .await
            .map_err(ChargeForUsageError::Repository)?
            .ok_or(ChargeForUsageError::NoPricing)?;
        if pricing.unit_price <= 0 {
            return Err(ChargeForUsageError::InvalidPricing(
                "active PAYG unit price must be positive",
            ));
        }

        let charge_units = input
            .units
            .checked_mul(pricing.unit_price)
            .ok_or(ChargeForUsageError::InvalidPricing("PAYG charge overflow"))?;

        // Workers can encounter a tenant that has never opened the wallet
        // page. Provisioning the tenant-scoped wallet here keeps metering
        // authoritative and avoids turning a valid first check into an
        // infrastructure error. Currency remains server-controlled by the
        // active PAYG price book.
        let wallet = self
            .wallet_repository
            .get_or_create(input.tenant_id, &pricing.currency)
            .await
            .map_err(ChargeForUsageError::Repository)?;
        if wallet.currency != pricing.currency {
            return Err(ChargeForUsageError::InvalidPricing(
                "PAYG pricing currency does not match the tenant wallet currency",
            ));
        }

        let expires_at = Utc::now() + Duration::seconds(60);
        let reservation_outcome = self
            .wallet_repository
            .reserve(
                input.tenant_id,
                charge_units,
                &input.operation,
                &input.request_id,
                expires_at,
            )
            .await
            .map_err(|e| {
                if e.to_string() == "insufficient wallet balance" {
                    ChargeForUsageError::InsufficientBalance
                } else {
                    ChargeForUsageError::Repository(e)
                }
            })?;

        if !reservation_outcome.created {
            // Another request with the same idempotency key owns the
            // reservation. Strict API callers wait; durable workers may resume
            // the same reservation with the recoverable variant below.
            if reservation_outcome.reservation.status == CreditReservation::CAPTURED {
                let event = self
                    .recover_captured_event(
                        &input,
                        &pricing.version,
                        charge_units,
                        &pricing.currency,
                        &pricing.unit_price,
                        reservation_outcome.reservation.id,
                    )
                    .await?;
                return Ok(ChargeForUsageResult {
                    usage_event: event,
                    transaction: None,
                });
            }
            if !retry_pending {
                return self
                    .wait_for_previous_result(
                        &input,
                        charge_units,
                        pricing.unit_price,
                        pricing.version,
                        &pricing.currency,
                    )
                    .await;
            }
            // For durable worker retries, resume the existing reservation below
            // instead of suppressing the operation. Wallet capture remains
            // idempotent, so concurrent recovery can never debit twice.
        }

        let reservation = reservation_outcome.reservation;
        if let Err(err) = operation().await {
            let _ = self.wallet_repository.release(reservation.id).await;
            let event = self
                .record_event(
                    &input,
                    0,
                    pricing.unit_price,
                    0,
                    pricing.version,
                    &pricing.currency,
                    "FAILED",
                )
                .await?;
            return Err(ChargeForUsageError::OperationFailed(format!(
                "{err}; usage_event={}",
                event.id
            )));
        }

        let event = self.build_event(
            &input,
            input.units,
            pricing.unit_price,
            charge_units,
            pricing.version,
            &pricing.currency,
            "CAPTURED",
        );
        let audits = self.build_payg_audits(&input, &event);
        let transaction = self
            .wallet_repository
            .capture(reservation.id, Some(&event), &audits)
            .await
            .map_err(ChargeForUsageError::CaptureFailed)?;
        // Postgres has already persisted these rows atomically with the debit.
        // The extra idempotent calls keep the in-memory adapter's observable
        // stores consistent for application/integration tests.
        let event = self
            .usage_event_repository
            .create(&event)
            .await
            .map_err(ChargeForUsageError::Repository)?;
        Ok(ChargeForUsageResult {
            usage_event: event,
            transaction: Some(transaction),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_event(
        &self,
        input: &ChargeForUsageInput,
        units_billed: i64,
        unit_price: i64,
        charge_units: i64,
        pricing_version: i64,
        currency: &str,
        status: &str,
    ) -> UsageEvent {
        UsageEvent {
            id: Uuid::new_v4(),
            tenant_id: input.tenant_id,
            operation: input.operation.clone(),
            units_requested: input.units,
            units_billed,
            unit_price,
            charge_units,
            currency: currency.to_owned(),
            pricing_version,
            request_id: input.request_id.clone(),
            status: status.to_owned(),
            created_at: Utc::now(),
        }
    }

    fn build_payg_audits(
        &self,
        input: &ChargeForUsageInput,
        event: &UsageEvent,
    ) -> Vec<domain::FinancialAuditLogEntry> {
        if self.audit_log_repository.is_none() {
            return Vec::new();
        }
        let metadata = serde_json::json!({
            "operation": input.operation, "units": input.units,
            "charge_units": event.charge_units, "currency": event.currency,
            "pricing_version": event.pricing_version,
        });
        ["payg_charged", "wallet_debited"]
            .into_iter()
            .map(|action| {
                domain::FinancialAuditLogEntry::new(
                    "system:payg",
                    Some(input.tenant_id),
                    action,
                    "payg_request",
                    Some(input.request_id.clone()),
                    metadata.clone(),
                )
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_event(
        &self,
        input: &ChargeForUsageInput,
        units_billed: i64,
        unit_price: i64,
        charge_units: i64,
        pricing_version: i64,
        currency: &str,
        status: &str,
    ) -> Result<UsageEvent, ChargeForUsageError> {
        let event = self.build_event(
            input,
            units_billed,
            unit_price,
            charge_units,
            pricing_version,
            currency,
            status,
        );
        self.usage_event_repository
            .create(&event)
            .await
            .map_err(ChargeForUsageError::Repository)
    }

    async fn recover_captured_event(
        &self,
        input: &ChargeForUsageInput,
        pricing_version: &i64,
        charge_units: i64,
        currency: &str,
        unit_price: &i64,
        reservation_id: Uuid,
    ) -> Result<UsageEvent, ChargeForUsageError> {
        if let Some(event) = self
            .usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id)
            .await
            .map_err(ChargeForUsageError::Repository)?
        {
            return Ok(event);
        }
        let event = self.build_event(
            input,
            input.units,
            *unit_price,
            charge_units,
            *pricing_version,
            currency,
            "CAPTURED",
        );
        let audits = self.build_payg_audits(input, &event);
        self.wallet_repository
            .capture(reservation_id, Some(&event), &audits)
            .await
            .map_err(ChargeForUsageError::CaptureFailed)?;
        self.usage_event_repository
            .create(&event)
            .await
            .map_err(ChargeForUsageError::Repository)
    }

    async fn wait_for_previous_result(
        &self,
        input: &ChargeForUsageInput,
        charge_units: i64,
        unit_price: i64,
        pricing_version: i64,
        currency: &str,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError> {
        for _ in 0..100 {
            if let Some(event) = self
                .usage_event_repository
                .find_by_request(input.tenant_id, &input.request_id)
                .await
                .map_err(ChargeForUsageError::Repository)?
            {
                return Ok(ChargeForUsageResult {
                    usage_event: event,
                    transaction: None,
                });
            }
            tokio::time::sleep(StdDuration::from_millis(50)).await;
        }
        let _ = (charge_units, unit_price, pricing_version, currency);
        Err(ChargeForUsageError::ReservationInProgress)
    }
}

fn validate_input(input: &ChargeForUsageInput) -> Result<(), ChargeForUsageError> {
    if input.units <= 0 {
        return Err(ChargeForUsageError::InvalidInput("units must be positive"));
    }
    if input.operation.trim().is_empty() {
        return Err(ChargeForUsageError::InvalidInput("operation is required"));
    }
    if input.request_id.trim().is_empty() {
        return Err(ChargeForUsageError::InvalidInput("request_id is required"));
    }
    Ok(())
}
PINGVERTEX_EOF

if [[ -f "crates/application/src/use_cases/meter_usage.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/src/use_cases/meter_usage.rs")"
  cp "crates/application/src/use_cases/meter_usage.rs" "$BACKUP_DIR/crates/application/src/use_cases/meter_usage.rs"
fi

mkdir -p "$(dirname "crates/application/src/use_cases/meter_usage.rs")"
cat > "crates/application/src/use_cases/meter_usage.rs" <<'PINGVERTEX_EOF'
use super::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase, MeteredAttempt,
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
                },
                operation,
            )
            .await
    }

    pub async fn find_by_request(&self, tenant_id: uuid::Uuid, request_id: &str) -> Result<Option<domain::UsageEvent>, ChargeForUsageError> {
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
                },
                operation,
            )
            .await
    }

    /// Runtime boundary with explicit attempted/not-attempted classification.
    pub async fn execute_classified_attempt<F, Fut, E>(
        &self, input: MeterUsageInput, operation: F,
    ) -> Result<(ChargeForUsageResult, MeteredAttempt), ChargeForUsageError>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<MeteredAttempt, E>>, E: fmt::Display
    {
        self.charge.execute_classified_attempt(
            ChargeForUsageInput { tenant_id: input.tenant_id, operation: input.operation, units: input.units, request_id: input.request_id },
            operation,
        ).await
    }

    pub async fn execute_classified_attempt_with_output<F, Fut, E, T>(
        &self, input: MeterUsageInput, operation: F,
    ) -> Result<(ChargeForUsageResult, Option<T>), ChargeForUsageError>
    where F: FnOnce() -> Fut, Fut: Future<Output = Result<(MeteredAttempt, T), E>>, E: fmt::Display
    {
        self.charge.execute_classified_attempt_with_output(
            ChargeForUsageInput { tenant_id: input.tenant_id, operation: input.operation, units: input.units, request_id: input.request_id },
            operation,
        ).await
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
                },
                operation,
            )
            .await
    }
}
PINGVERTEX_EOF

if [[ -f "crates/application/src/use_cases/mod.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/src/use_cases/mod.rs")"
  cp "crates/application/src/use_cases/mod.rs" "$BACKUP_DIR/crates/application/src/use_cases/mod.rs"
fi

mkdir -p "$(dirname "crates/application/src/use_cases/mod.rs")"
cat > "crates/application/src/use_cases/mod.rs" <<'PINGVERTEX_EOF'
#![allow(ambiguous_glob_reexports)]
pub mod accept_invitation;
pub mod acknowledge_incident;
pub mod add_member;
pub mod begin_grace_period;
pub mod billing_support;
pub mod billing_webhook_support;
pub mod cancel_subscription;
pub mod change_member_role;
pub mod change_password;
pub mod confirm_totp_setup;
pub mod create_api_flow;
pub mod create_api_key;
pub mod create_email_verification_token;
pub mod create_escalation_policy;
pub mod create_invitation;
pub mod create_maintenance_window;
pub mod create_monitor;
pub mod create_notification_channel;
pub mod create_status_page;
pub mod delete_api_flow;
pub mod delete_api_key;
pub mod delete_monitor;
pub mod delete_notification_channel;
pub mod delete_status_page;
pub mod disable_totp;
pub mod email_verification_support;
pub mod enable_totp;
pub mod expire_subscription;
pub mod finalize_scheduled_cancellation;
pub mod get_dashboard_overview;
pub mod get_monitor;
pub mod get_proxy_check_job;
pub mod initiate_checkout;
pub mod invitation_common;
pub mod invitation_support;
pub mod list_api_keys;
pub mod list_checks;
pub mod list_incidents;
pub mod list_invitations;
pub mod list_members;
pub mod list_monitor_checks;
pub mod list_monitors;
pub mod list_notification_channels;
pub mod list_status_pages;
pub mod login_user;
pub mod membership_common;
pub mod oauth_login_user;
pub mod process_billing_webhook;
pub mod register_user;
pub mod remove_custom_domain;
pub mod remove_member;
pub mod request_custom_domain;
pub mod request_password_reset;
pub mod resend_verification;
pub mod reset_password;
pub mod reset_password_support;
pub mod resolve_entitlements;
pub mod revoke_invitation;
pub mod send_test_notification;
pub mod session_invalidation;
pub mod submit_proxy_check_job;
pub mod subscription_lifecycle_support;
pub mod totp_support;
pub mod update_api_flow;
pub mod update_invoice_status;
pub mod update_monitor;
pub mod update_monitor_enabled;
pub mod update_status_page;
pub mod upload_status_page_logo;
pub mod verify_checkout;
pub mod verify_email;
pub mod verify_totp_and_login;

// H14 — Help Center & Support:
pub mod create_support_ticket;
pub mod get_help_article;
pub mod get_support_ticket;
pub mod list_help_categories;
pub mod list_my_support_tickets;
pub mod reply_to_support_ticket;
pub mod search_help_articles;
pub mod update_support_ticket_status;

pub use accept_invitation::*;
pub use acknowledge_incident::*;
pub use add_member::*;
pub use begin_grace_period::*;
pub use billing_support::*;
pub use billing_webhook_support::*;
pub use cancel_subscription::*;
pub use change_member_role::*;
pub use change_password::*;
pub use confirm_totp_setup::*;
pub use create_api_flow::*;
pub use create_api_key::*;
pub use create_email_verification_token::*;
pub use create_escalation_policy::*;
pub use create_invitation::*;
pub use create_maintenance_window::*;
pub use create_monitor::*;
pub use create_notification_channel::*;
pub use create_status_page::*;
pub use delete_api_flow::*;
pub use delete_api_key::*;
pub use delete_monitor::*;
pub use delete_notification_channel::*;
pub use delete_status_page::*;
pub use disable_totp::*;
pub use email_verification_support::*;
pub use enable_totp::*;
pub use expire_subscription::*;
pub use finalize_scheduled_cancellation::*;
pub use get_dashboard_overview::*;
pub use get_monitor::*;
pub use get_proxy_check_job::*;
pub use initiate_checkout::*;
pub use invitation_common::*;
pub use invitation_support::*;
pub use list_api_keys::*;
pub use list_checks::*;
pub use list_incidents::*;
pub use list_invitations::*;
pub use list_members::*;
pub use list_monitor_checks::*;
pub use list_monitors::*;
pub use list_notification_channels::*;
pub use list_status_pages::*;
pub use login_user::*;
pub use membership_common::*;
pub use oauth_login_user::*;
pub use process_billing_webhook::*;
pub use register_user::*;
pub use remove_custom_domain::*;
pub use remove_member::*;
pub use request_custom_domain::*;
pub use request_password_reset::*;
pub use resend_verification::*;
pub use reset_password::*;
pub use reset_password_support::*;
pub use resolve_entitlements::*;
pub use revoke_invitation::*;
pub use send_test_notification::*;
pub use session_invalidation::*;
pub use submit_proxy_check_job::*;
pub use subscription_lifecycle_support::*;
pub use update_api_flow::*;
pub use update_invoice_status::*;
pub use update_monitor::*;
pub use update_monitor_enabled::*;
pub use update_status_page::*;
pub use upload_status_page_logo::*;
pub use verify_checkout::*;
pub use verify_email::*;
pub use verify_totp_and_login::*;
pub mod calculate_pricing_quote;

// H12 — Proration Service:
pub mod calculate_subscription_cycle_cost;
pub mod update_subscription_usage;
pub use calculate_subscription_cycle_cost::*;
pub use update_subscription_usage::*;

pub mod add_on_subscription;
pub mod change_subscription_plan;
pub mod generate_invoice;
pub mod verify_addon_checkout;
pub use change_subscription_plan::*;
pub use generate_invoice::*;
pub use verify_addon_checkout::*;

// H14 — Help Center & Support:
pub use create_support_ticket::{CreateSupportTicketError, CreateSupportTicketUseCase};
pub use get_help_article::GetHelpArticleUseCase;
pub use get_support_ticket::GetSupportTicketUseCase;
pub use list_help_categories::ListHelpCategoriesUseCase;
pub use list_my_support_tickets::ListMySupportTicketsUseCase;
pub use reply_to_support_ticket::ReplyToSupportTicketUseCase;
pub use search_help_articles::SearchHelpArticlesUseCase;
pub use update_support_ticket_status::UpdateSupportTicketStatusUseCase;

// RUM (Real User Monitoring):
pub mod ingest_rum_event;
pub mod manage_rum_sites;
pub mod rum_dashboard_queries;
pub use ingest_rum_event::{IngestRumEventError, IngestRumEventUseCase};
pub use manage_rum_sites::{CreateRumSiteError, CreateRumSiteUseCase, ListRumSitesUseCase};
pub use rum_dashboard_queries::{RumDashboardError, RumDashboardQueriesUseCase};

pub mod verify_email_otp;
pub use verify_email_otp::{VerifyEmailOtpError, VerifyEmailOtpUseCase};

pub mod cancel_proxy_check_job;
pub mod list_proxy_check_results;
pub use cancel_proxy_check_job::*;
pub use list_proxy_check_results::*;

pub mod charge_for_usage;
pub mod meter_usage;
pub use charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase, MeteredAttempt,
};
pub use meter_usage::{MeterUsageInput, MeterUsageUseCase};
PINGVERTEX_EOF

if [[ -f "crates/worker/src/durable_scheduler.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/durable_scheduler.rs")"
  cp "crates/worker/src/durable_scheduler.rs" "$BACKUP_DIR/crates/worker/src/durable_scheduler.rs"
fi

mkdir -p "$(dirname "crates/worker/src/durable_scheduler.rs")"
cat > "crates/worker/src/durable_scheduler.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc, time::Duration};

use application::ports::SchedulingRepository;
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use async_trait::async_trait;
use chrono::Utc;
use domain::Monitor;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct CheckExecution {
    pub attempted: bool,
    pub error: Option<String>,
}

impl CheckExecution {
    pub fn success(attempted: bool) -> Self { Self { attempted, error: None } }
    pub fn failed(attempted: bool, error: impl Into<String>) -> Self { Self { attempted, error: Some(error.into()) } }
}

/// Executes the actual network check for a claimed monitor. B17 ships a
/// stub implementation; B18/B19 will replace it with the real HTTP/TCP/
/// DNS/heartbeat check logic and persistence.
#[async_trait]
pub trait CheckExecutor: Send + Sync + Clone + 'static {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution>;
}

/// Blanket implementation so `Arc<T>` can be passed to the scheduler
/// without a special wrapper type.
#[async_trait]
impl<T> CheckExecutor for Arc<T>
where
    T: CheckExecutor + Send + Sync + 'static,
{
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        (**self).execute(monitor).await
    }
}

/// B17 placeholder executor. Logs and succeeds without doing any check
/// work, satisfying the durable scheduler contract without stepping on
/// B18/B19's scope.
#[derive(Clone)]
pub struct StubCheckExecutor;

#[async_trait]
impl CheckExecutor for StubCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        info!(
            monitor_id = %monitor.id.0,
            "B17 placeholder check executor: not performing real check yet"
        );
        Ok(CheckExecution::success(false))
    }
}

pub struct DurableScheduler<R, E> {
    scheduling_repository: R,
    executor: E,
    lease_duration: Duration,
    max_concurrency: usize,
    lease_owner: Uuid,
    metering: Option<Arc<MeterUsageUseCase>>,
}

impl<R, E> DurableScheduler<R, E>
where
    R: SchedulingRepository + Clone + Send + Sync + 'static,
    E: CheckExecutor,
{
    pub fn new(
        scheduling_repository: R,
        executor: E,
        lease_duration: Duration,
        max_concurrency: usize,
    ) -> Self {
        Self {
            scheduling_repository,
            executor,
            lease_duration,
            max_concurrency,
            lease_owner: Uuid::new_v4(),
            metering: None,
        }
    }

    /// Enables PAYG billing for durable monitor executions. The scheduler
    /// supplies the stable execution identity; metering finalizes the charge
    /// after the actual attempt, including failed target/network attempts.
    pub fn with_metering(mut self, metering: Arc<MeterUsageUseCase>) -> Self {
        self.metering = Some(metering);
        self
    }

    pub async fn run(self) -> Result<()> {
        info!("Durable scheduler started");
        let mut tasks: JoinSet<Result<()>> = JoinSet::new();

        loop {
            self.scheduling_repository
                .recover_expired_leases(Utc::now())
                .await?;

            let free_capacity = self.max_concurrency.saturating_sub(tasks.len());
            if free_capacity > 0 {
                let claimed = self
                    .scheduling_repository
                    .claim_due_monitors(
                        free_capacity as u32,
                        self.lease_owner,
                        self.lease_duration,
                        Utc::now(),
                    )
                    .await?;

                info!(count = claimed.len(), "claimed due monitors");

                for claim in claimed {
                    let scheduling_repository = self.scheduling_repository.clone();
                    let executor = self.executor.clone();
                    let metering = self.metering.clone();
                    let lease_owner = self.lease_owner;

                    tasks.spawn(async move {
                        let monitor = claim.monitor;
                        let request_id =
                            format!("monitor-check:{}:{}", monitor.id.0, claim.execution_id);
                        let result = if let Some(metering) = metering {
                            match metering.execute_classified_attempt(
                                MeterUsageInput {
                                    tenant_id: monitor.tenant_id.0,
                                    operation: "monitor_check".to_owned(),
                                    units: 1,
                                    request_id,
                                },
                                || {
                                    let executor = executor.clone();
                                    let monitor = monitor.clone();
                                    async move {
                                        executor.execute(&monitor).await.map(|outcome| {
                                            MeteredAttempt {
                                                attempted: outcome.attempted,
                                                error: outcome.error,
                                            }
                                        })
                                    }
                                },
                            ).await {
                                Ok(_) => Ok(()),
                                Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => {
                                    warn!(monitor_id=%monitor.id.0, reason=%reason, "monitor execution did not reach a billable attempt");
                                    Ok(())
                                }
                                Err(err) => Err(Box::new(err) as Box<dyn Error + Send + Sync>),
                            }
                        } else {
                            executor.execute(&monitor).await.map(|_| ()).map_err(|err| err)
                        };
                        if let Err(err) = &result {
                            warn!(
                                monitor_id = %monitor.id.0,
                                error = %err,
                                "check execution failed"
                            );
                        }

                        // Always release the claim after an execution attempt.
                        // If the process crashes before this point, the lease
                        // will expire and `recover_expired_leases` will make
                        // the monitor claimable again.
                        if let Err(err) = scheduling_repository
                            .complete_claim(monitor.id, lease_owner)
                            .await
                        {
                            error!(
                                monitor_id = %monitor.id.0,
                                error = %err,
                                "failed to release claim"
                            );
                        }

                        result
                    });
                }
            }

            if tasks.is_empty() {
                // No due monitors and no running tasks; avoid a hot loop.
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                // Wait for at least one task to finish before the next
                // claim/recovery iteration. This bounds concurrency.
                match tasks.join_next().await {
                    Some(Ok(Ok(_))) => {}
                    Some(Ok(Err(err))) => {
                        warn!(error = %err, "monitor check task returned an error");
                    }
                    Some(Err(join_err)) => {
                        error!(error = %join_err, "monitor check task panicked");
                    }
                    None => {}
                }
            }
        }
    }
}

/// Convenience entry point used by the worker binary.
pub async fn run_durable<R, E>(
    scheduling_repository: R,
    executor: E,
    lease_duration: Duration,
    max_concurrency: usize,
) -> Result<()>
where
    R: SchedulingRepository + Clone + Send + Sync + 'static,
    E: CheckExecutor,
{
    DurableScheduler::new(
        scheduling_repository,
        executor,
        lease_duration,
        max_concurrency,
    )
    .run()
    .await
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/checker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/checker.rs")"
  cp "crates/worker/src/checker.rs" "$BACKUP_DIR/crates/worker/src/checker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/checker.rs")"
cat > "crates/worker/src/checker.rs" <<'PINGVERTEX_EOF'
use std::{
    collections::HashMap,
    error::Error as StdError,
    net::SocketAddr,
    time::{Duration, Instant},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
};

use chrono::{DateTime, Utc};
use domain::{CheckErrorKind, CheckResult, HttpMethod, Monitor, RegionCode};
use reqwest::Client;
use shared::ssrf::ResolvedDestination;
use tracing::warn;

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};
use crate::diagnostics::run_diagnostics;

type Result = std::result::Result<CheckResult, Box<dyn StdError + Send + Sync>>;

/// Maximum number of response-body bytes an HTTP check will ever read.
/// The checker never reads an unbounded response body; if a response is
/// larger than this, the check fails with `CheckErrorKind::HttpError`.
const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 1024 * 1024;

/// The result of an HTTP/HTTPS check execution plus its canonical error
/// classification. `CheckErrorKind` is the shared taxonomy from B18 and
/// is the field every active check worker must populate.
#[derive(Debug, Clone)]
pub struct HttpCheckOutcome {
    pub result: CheckResult,
    pub error_kind: CheckErrorKind,
    pub attempted: bool,
}

impl HttpCheckOutcome {
    fn new(result: CheckResult, error_kind: CheckErrorKind) -> Self {
        Self { result, error_kind, attempted: true }
    }

    fn failure(
        region: RegionCode,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self { result: CheckResult::failure(region, latency_ms, error), error_kind, attempted: false }
    }

    fn failure_attempted(
        region: RegionCode, latency_ms: u128, error_kind: CheckErrorKind, error: impl Into<String>,
    ) -> Self {
        Self { result: CheckResult::failure(region, latency_ms, error), error_kind, attempted: true }
    }

    fn failure_with_status(
        region: RegionCode,
        status: u16,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self { result: CheckResult::failure_with_status(region, status, latency_ms, error), error_kind, attempted: true }
    }
}

/// Fetches TLS certificate expiry info for `monitor`'s endpoint, if
/// `monitor.ssl_check_enabled` is set and `monitor.url` is `https://`.
/// Otherwise returns `(None, None)` immediately without any network I/O.
///
/// `hostname` and `socket_addr` MUST already have been through SSRF
/// validation by the caller.
pub async fn maybe_fetch_cert_info(
    monitor: &Monitor,
    hostname: &str,
    socket_addr: SocketAddr,
) -> (Option<DateTime<Utc>>, Option<i64>) {
    if !monitor.ssl_check_enabled || !monitor.url.as_str().starts_with("https://") {
        return (None, None);
    }

    let timeout_duration = Duration::from_secs(monitor.timeout_seconds.into());

    match crate::tls_cert::fetch_certificate_info(hostname, socket_addr, timeout_duration).await {
        Ok(info) => {
            if info.days_remaining <= monitor.ssl_expiry_warning_days as i64 {
                warn!(
                    monitor_id = %monitor.id.0,
                    name = %monitor.name.as_str(),
                    days_remaining = info.days_remaining,
                    warning_threshold_days = monitor.ssl_expiry_warning_days,
                    "TLS certificate is expiring soon"
                );
            }

            (Some(info.expires_at), Some(info.days_remaining))
        }
        Err(err) => {
            warn!(
                monitor_id = %monitor.id.0,
                name = %monitor.name.as_str(),
                error = %err,
                "failed to fetch TLS certificate info; continuing without cert data"
            );

            (None, None)
        }
    }
}

/// Performs a real HTTP/HTTPS check with SSRF-safe DNS pinning and a
/// total-timeout budget. This is the production path used by
/// `HttpCheckExecutor`.
pub async fn check_http(monitor: &Monitor) -> HttpCheckOutcome {
    if !monitor.enabled {
        return HttpCheckOutcome::failure(
            RegionCode::default_region(),
            0,
            CheckErrorKind::Unknown,
            "monitor is disabled",
        );
    }

    let timeout_duration = Duration::from_secs(monitor.timeout_seconds.into());
    let overall_started = Instant::now();
    let attempt_started = Arc::new(AtomicBool::new(false));

    match tokio::time::timeout(timeout_duration, check_http_real(monitor, Arc::clone(&attempt_started))).await {
        Ok(outcome) => outcome,
        Err(_) => {
            let mut outcome = HttpCheckOutcome::failure(
                RegionCode::default_region(),
                overall_started.elapsed().as_millis(),
                CheckErrorKind::Timeout,
                "HTTP check timed out",
            );
            outcome.attempted = attempt_started.load(Ordering::Acquire);

            let diag = run_diagnostics(monitor, None, CheckErrorKind::Timeout).await;
            outcome.result = outcome
                .result
                .with_diagnostics(diag.steps, diag.root_cause_summary);
            outcome
        }
    }
}

/// SSRF-safe, DNS-pinned HTTP/HTTPS check body. The caller wraps this in
/// `tokio::time::timeout` so DNS + TCP + TLS + response are all bounded
/// by the monitor's single timeout budget.
async fn check_http_real(monitor: &Monitor, attempt_started: Arc<AtomicBool>) -> HttpCheckOutcome {
    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_destination(&monitor.url).await {
        Ok(destination) => destination,
        Err(err) => {
            let latency_ms = started.elapsed().as_millis();
            let mut outcome = HttpCheckOutcome::failure(
                RegionCode::default_region(),
                latency_ms,
                CheckErrorKind::DnsError,
                err,
            );

            let diag = run_diagnostics(monitor, None, CheckErrorKind::DnsError).await;
            outcome.result = outcome
                .result
                .with_diagnostics(diag.steps, diag.root_cause_summary);
            return outcome;
        }
    };

    let pinned_client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        // SECURITY: without this, reqwest honors HTTP_PROXY/HTTPS_PROXY/
        // ALL_PROXY from the process environment and hands the request's
        // destination host+port to that proxy verbatim — bypassing the
        // `.resolve()` DNS pinning above entirely, since the proxy (not
        // this process) does its own DNS resolution. That reopens the
        // exact SSRF/DNS-rebinding hole `resolve_and_validate_destination`
        // exists to close: a monitor target could resolve to a public IP
        // at validation time and a private one at proxy-connect time.
        .no_proxy()
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            let latency_ms = started.elapsed().as_millis();
            let mut outcome = HttpCheckOutcome::failure(
                RegionCode::default_region(),
                latency_ms,
                CheckErrorKind::InternalError,
                err.to_string(),
            );

            let diag =
                run_diagnostics(monitor, Some(&destination), CheckErrorKind::InternalError).await;
            outcome.result = outcome
                .result
                .with_diagnostics(diag.steps, diag.root_cause_summary);
            return outcome;
        }
    };

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let request_started = Instant::now();
    attempt_started.store(true, Ordering::Release);
    let response_result = pinned_client
        .request(method, destination.url.clone())
        .send()
        .await;

    let mut outcome = handle_http_response(
        response_result,
        request_started.elapsed().as_millis(),
        monitor,
        true,
        Some(&destination),
    )
    .await;

    if !outcome.result.success {
        let failure_kind = outcome.error_kind;
        let diag = run_diagnostics(monitor, Some(&destination), failure_kind).await;
        outcome.result = outcome
            .result
            .with_diagnostics(diag.steps, diag.root_cause_summary);
    }

    outcome
}

/// Backwards-compatible HTTP check entry point used by the legacy
/// sweep scheduler (`scheduler.rs`).
pub async fn check_monitor(_client: &Client, monitor: &Monitor) -> Result {
    Ok(check_http(monitor).await.result)
}

/// Test-friendly HTTP check that performs the actual HTTP/HTTPS request,
/// but uses an injected destination validator instead of the real SSRF
/// resolver. The response is read with the same bounded-body guard as the
/// production path.
pub async fn check_http_with_validator<F, Fut>(
    client: &Client,
    monitor: &Monitor,
    validate_destination: F,
) -> std::result::Result<HttpCheckOutcome, Box<dyn StdError + Send + Sync>>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), Box<dyn StdError + Send + Sync>>>,
{
    if !monitor.enabled {
        return Ok(HttpCheckOutcome::failure(
            RegionCode::default_region(),
            0,
            CheckErrorKind::Unknown,
            "monitor is disabled",
        ));
    }

    validate_destination(monitor.url.as_str().to_string()).await?;

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let started = Instant::now();
    let response_result = client
        .request(method, monitor.url.as_str())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    Ok(handle_http_response(
        response_result,
        started.elapsed().as_millis(),
        monitor,
        true,
        None,
    )
    .await)
}

/// Backwards-compatible validator path used by existing tests in
/// `crates/worker/tests/checker.rs`. Unlike `check_http_with_validator`,
/// an unexpected HTTP status is represented without a `status_code` on the
/// legacy `CheckResult` (preserving pre-B18 test expectations). All other
/// behavior, including bounded body reading, is identical.
pub async fn check_monitor_with_validator<F, Fut>(
    client: &Client,
    monitor: &Monitor,
    validate_destination: F,
) -> Result
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), Box<dyn StdError + Send + Sync>>>,
{
    if !monitor.enabled {
        return Ok(CheckResult::failure(
            RegionCode::default_region(),
            0,
            "monitor is disabled",
        ));
    }

    validate_destination(monitor.url.as_str().to_string()).await?;

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let started = Instant::now();
    let response_result = client
        .request(method, monitor.url.as_str())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    Ok(handle_http_response(
        response_result,
        started.elapsed().as_millis(),
        monitor,
        false,
        None,
    )
    .await
    .result)
}

/// Shared response evaluation for both the production and validator paths.
///
/// `include_status_on_mismatch` controls whether a wrong HTTP status is
/// persisted on `CheckResult.status_code`. The B18 production path always
/// captures the actual status (even when the check fails), while the legacy
/// validator path keeps its historical shape for existing tests.
async fn handle_http_response(
    response_result: reqwest::Result<reqwest::Response>,
    latency_ms: u128,
    monitor: &Monitor,
    include_status_on_mismatch: bool,
    destination: Option<&ResolvedDestination>,
) -> HttpCheckOutcome {
    match response_result {
        Ok(response) => {
            let status = response.status().as_u16();

            if status != monitor.expected_status {
                let error_msg = format!("expected {}, got {}", monitor.expected_status, status);
                let result = if include_status_on_mismatch {
                    CheckResult::failure_with_status(
                        RegionCode::default_region(),
                        status,
                        latency_ms,
                        error_msg,
                    )
                } else {
                    CheckResult::failure(RegionCode::default_region(), latency_ms, error_msg)
                };

                return HttpCheckOutcome::new(result, CheckErrorKind::HttpError);
            }

            let (headers, body_bytes) = if monitor.assertions.is_empty() {
                (HashMap::new(), Vec::new())
            } else {
                let headers = collect_headers(&response);
                match read_body_bounded(response).await {
                    Ok(body) => (headers, body),
                    Err(error_kind) => {
                        return HttpCheckOutcome::failure_with_status(
                            RegionCode::default_region(),
                            status,
                            latency_ms,
                            error_kind,
                            "HTTP response body exceeded the maximum permitted size",
                        );
                    }
                }
            };

            if !monitor.assertions.is_empty() {
                let ctx = EvalContext {
                    status_code: status,
                    headers,
                    body_bytes,
                    latency_ms,
                };

                let (all_passed, results) = evaluate_assertions(&monitor.assertions, &ctx);

                if !all_passed {
                    let errors: Vec<String> =
                        results.iter().filter_map(|r| r.message.clone()).collect();
                    let error_msg = format!("assertions sun gaza: {}", errors.join("; "));

                    return HttpCheckOutcome::failure_with_status(
                        RegionCode::default_region(),
                        status,
                        latency_ms,
                        CheckErrorKind::AssertionFailed,
                        error_msg,
                    );
                }
            }

            let (cert_expires_at, cert_days_remaining) = match destination {
                Some(destination) => {
                    maybe_fetch_cert_info(monitor, &destination.host, destination.socket_addr).await
                }
                None => (None, None),
            };

            HttpCheckOutcome::new(
                CheckResult::success(RegionCode::default_region(), status, latency_ms)
                    .with_cert_info(cert_expires_at, cert_days_remaining),
                CheckErrorKind::Unknown,
            )
        }

        Err(err) => {
            let error_kind = classify_reqwest_error(&err);
            HttpCheckOutcome::failure(
                RegionCode::default_region(),
                latency_ms,
                error_kind,
                err.to_string(),
            )
        }
    }
}

fn collect_headers(response: &reqwest::Response) -> HashMap<String, String> {
    response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// Reads the response body up to `MAX_HTTP_RESPONSE_BODY_BYTES`. Never
/// reads unbounded and never accepts more than the configured cap.
async fn read_body_bounded(
    mut response: reqwest::Response,
) -> std::result::Result<Vec<u8>, CheckErrorKind> {
    let mut body = Vec::new();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > MAX_HTTP_RESPONSE_BODY_BYTES {
                    return Err(CheckErrorKind::HttpError);
                }

                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(err) => return Err(classify_reqwest_error(&err)),
        }
    }

    Ok(body)
}

fn classify_reqwest_error(err: &reqwest::Error) -> CheckErrorKind {
    if err.is_timeout() {
        CheckErrorKind::Timeout
    } else if is_tls_error(err) {
        CheckErrorKind::TlsError
    } else if err.is_connect() {
        CheckErrorKind::ConnectionError
    } else if err.is_body() {
        CheckErrorKind::HttpError
    } else {
        CheckErrorKind::Unknown
    }
}

fn is_tls_error(err: &reqwest::Error) -> bool {
    let mut source = StdError::source(err);

    while let Some(err) = source {
        let msg = err.to_string();

        if msg.contains("tls")
            || msg.contains("TLS")
            || msg.contains("ssl")
            || msg.contains("SSL")
            || msg.contains("certificate")
        {
            return true;
        }

        source = err.source();
    }

    false
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/tcp_checker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/tcp_checker.rs")"
  cp "crates/worker/src/tcp_checker.rs" "$BACKUP_DIR/crates/worker/src/tcp_checker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/tcp_checker.rs")"
cat > "crates/worker/src/tcp_checker.rs" <<'PINGVERTEX_EOF'
use std::time::{Duration, Instant};

use domain::{CheckErrorKind, CheckResult, Monitor, RegionCode};
use shared::ssrf::resolve_and_validate_host_port;
use tokio::net::TcpStream;
use tokio::time::timeout;
use url::Url;

type Result = std::result::Result<CheckResult, Box<dyn std::error::Error + Send + Sync>>;
type OutcomeResult = std::result::Result<TcpCheckOutcome, Box<dyn std::error::Error + Send + Sync>>;

/// The result of a TCP check execution plus its canonical error
/// classification. Mirrors `HttpCheckOutcome` so active check executors
/// can persist the exact same `ErrorType` taxonomy.
#[derive(Debug, Clone)]
pub struct TcpCheckOutcome {
    pub result: CheckResult,
    pub error_kind: CheckErrorKind,
    pub attempted: bool,
}

impl TcpCheckOutcome {
    fn success(region: RegionCode, latency_ms: u128) -> Self {
        Self {
            result: CheckResult::success(region, 0, latency_ms),
            error_kind: CheckErrorKind::Unknown,
            attempted: true,
        }
    }

    fn failure_attempted(
        region: RegionCode, latency_ms: u128, error_kind: CheckErrorKind, error: impl Into<String>,
    ) -> Self {
        Self { result: CheckResult::failure(region, latency_ms, error), error_kind, attempted: true }
    }

    fn failure(
        region: RegionCode,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self { result: CheckResult::failure(region, latency_ms, error), error_kind, attempted: false }
    }
}

/// Parses a host from `monitor.url`. TCP/DNS monitors store the target in
/// `monitor.url` as a full URL (e.g. `http://example.com`), but the SSRF
/// helpers and TCP/DNS checkers need a bare host. If `monitor.url` is
/// already a bare host, it is returned unchanged.
fn extract_host(raw: &str) -> String {
    if raw.contains("://")
        && let Ok(parsed) = Url::parse(raw)
        && let Some(host) = parsed.host_str()
    {
        return host.to_string();
    }
    raw.to_string()
}

/// Performs a TCP connectivity check for a `MonitorKind::Tcp` monitor.
///
/// SECURITY: this is NOT a general-purpose port scanner. The destination
/// host/port is resolved and validated through the SAME SSRF guard every
/// other checker in this codebase uses (`shared::ssrf`), and the connection
/// is pinned to the validated, already-resolved `SocketAddr` — a second DNS
/// lookup at connect time (which could resolve differently, e.g. DNS
/// rebinding) never happens.
pub async fn check_tcp_outcome(monitor: &Monitor) -> OutcomeResult {
    let port = monitor
        .tcp_port
        .ok_or("tcp_port is missing for a Tcp monitor")?;
    let host = extract_host(&monitor.url);

    let started = Instant::now();

    let socket_addr = match resolve_and_validate_host_port(&host, port).await {
        Ok(addr) => addr,
        Err(err) => {
            return Ok(TcpCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::DnsError,
                err,
            ));
        }
    };

    let timeout_duration = Duration::from_secs(monitor.timeout_seconds.into());
    let connect_result = timeout(timeout_duration, TcpStream::connect(socket_addr)).await;

    let latency_ms = started.elapsed().as_millis();

    match connect_result {
        Ok(Ok(_stream)) => Ok(TcpCheckOutcome::success(
            RegionCode::default_region(),
            latency_ms,
        )),
        Ok(Err(err)) => Ok(TcpCheckOutcome::failure_attempted(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::ConnectionError,
            format!("TCP connection failed: {err}"),
        )),
        Err(_) => Ok(TcpCheckOutcome::failure_attempted(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::Timeout,
            "TCP connection timed out",
        )),
    }
}

/// Backwards-compatible TCP entry point used by the legacy sweep
/// scheduler (`scheduler.rs`).
pub async fn check_tcp(monitor: &Monitor) -> Result {
    let outcome = check_tcp_outcome(monitor).await?;
    Ok(outcome.result)
}

/// Direct TCP connect helper (SSRF validation and DNS pinning have
/// already been performed by the caller). Public only for tests.
#[doc(hidden)]
pub async fn connect_tcp(
    socket_addr: std::net::SocketAddr,
    timeout_duration: Duration,
) -> std::result::Result<(), std::io::Error> {
    match timeout(timeout_duration, TcpStream::connect(socket_addr)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "TCP connection timed out",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::shared::ids::{MonitorId, TenantId, UserId};
    use domain::{
        CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorKind, MonitorName,
        Timeout, Url,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::net::TcpListener;
    use uuid::Uuid;

    fn build_tcp_monitor(url: &str, port: u16) -> Monitor {
        MonitorBuilder::new(
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId::new(),
        )
        .name(MonitorName::new("TCP Test").unwrap())
        .url(Url::new(format!("http://{url}")).unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(2).unwrap())
        .kind(MonitorKind::Tcp)
        .tcp_port(port)
        .try_build()
        .unwrap()
    }

    #[tokio::test]
    async fn rejects_private_ip_target() {
        let result = shared::ssrf::resolve_and_validate_host_port("10.0.0.5", 5432).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn missing_tcp_port_is_an_error() {
        let mut monitor = build_tcp_monitor("example.com", 443);
        monitor.tcp_port = None;
        let result = check_tcp_outcome(&monitor).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn successful_connection_returns_up() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();

        tokio::spawn(async move { if let Ok((_stream, _)) = listener.accept().await {} });

        let result = connect_tcp(addr, Duration::from_secs(2)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn refused_connection_returns_error() {
        // Bind a listener to obtain a free port, then drop it so the port
        // is closed and the next connect attempt should be refused.
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);

        let result = connect_tcp(addr, Duration::from_secs(2)).await;
        assert!(result.is_err());
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/dns_checker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/dns_checker.rs")"
  cp "crates/worker/src/dns_checker.rs" "$BACKUP_DIR/crates/worker/src/dns_checker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/dns_checker.rs")"
cat > "crates/worker/src/dns_checker.rs" <<'PINGVERTEX_EOF'
use std::time::{Duration, Instant};

use domain::{CheckErrorKind, CheckResult, Monitor, RegionCode};
use hickory_resolver::{
    Resolver, TokioResolver,
    config::{CLOUDFLARE, ResolverConfig},
    net::{DnsError, NetError, runtime::TokioRuntimeProvider},
    proto::rr::{RData, RecordType},
};
use tokio::time::timeout;
use url::Url;

type Result = std::result::Result<CheckResult, Box<dyn std::error::Error + Send + Sync>>;
type OutcomeResult = std::result::Result<DnsCheckOutcome, Box<dyn std::error::Error + Send + Sync>>;

/// The result of a DNS check execution plus its canonical error
/// classification. Mirrors `HttpCheckOutcome` so active check executors
/// can persist the exact same `ErrorType` taxonomy.
#[derive(Debug, Clone)]
pub struct DnsCheckOutcome {
    pub result: CheckResult,
    pub error_kind: CheckErrorKind,
    pub attempted: bool,
}

impl DnsCheckOutcome {
    fn success(region: RegionCode, latency_ms: u128) -> Self {
        Self {
            result: CheckResult::success(region, 0, latency_ms),
            error_kind: CheckErrorKind::Unknown,
            attempted: true,
        }
    }

    fn failure(
        region: RegionCode,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self { result: CheckResult::failure(region, latency_ms, error), error_kind, attempted: true }
    }
}

fn extract_host(raw: &str) -> String {
    if raw.contains("://")
        && let Ok(parsed) = Url::parse(raw)
        && let Some(host) = parsed.host_str()
    {
        return host.to_string();
    }
    raw.to_string()
}

/// Performs a DNS lookup check for a `MonitorKind::Dns` monitor.
///
/// SECURITY: always uses a hardcoded PUBLIC resolver (Cloudflare), never
/// the host's `/etc/resolv.conf` — this avoids depending on whatever DNS
/// infrastructure happens to be configured on the worker's network, which
/// could be internal/split-horizon DNS.
///
/// TIMEOUT: the entire lookup is wrapped in `tokio::time::timeout` using
/// the monitor's configured timeout, so a slow/blackholed resolver cannot
/// hold a worker task past the monitor's budget.
pub async fn check_dns_outcome(monitor: &Monitor) -> OutcomeResult {
    let record_type_str = monitor
        .dns_record_type
        .as_deref()
        .ok_or("dns_record_type is missing for a Dns monitor")?;
    let record_type = parse_record_type(record_type_str)?;

    let resolver = build_public_resolver()?;
    let host = extract_host(&monitor.url);
    let started = Instant::now();

    let lookup_result = timeout(
        Duration::from_secs(monitor.timeout_seconds.into()),
        resolver.lookup(host.as_str(), record_type),
    )
    .await;

    let lookup = match lookup_result {
        Err(_) => {
            return Ok(DnsCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::Timeout,
                "DNS lookup timed out",
            ));
        }
        Ok(Err(NetError::Dns(DnsError::NoRecordsFound(_)))) => {
            return Ok(DnsCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::DnsError,
                format!("no {record_type_str} records found for '{host}'"),
            ));
        }
        Ok(Err(err)) => {
            return Ok(DnsCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::DnsError,
                format!("DNS lookup failed: {err}"),
            ));
        }
        Ok(Ok(lookup)) => lookup,
    };

    let latency_ms = started.elapsed().as_millis();

    let answers: Vec<String> = lookup
        .answers()
        .iter()
        .map(|record| rdata_to_string(&record.data))
        .collect();

    match evaluate_dns_answers(
        &answers,
        monitor.dns_expected_value.as_deref(),
        &host,
        record_type_str,
    ) {
        Ok(true) => Ok(DnsCheckOutcome::success(
            RegionCode::default_region(),
            latency_ms,
        )),
        Ok(false) => Ok(DnsCheckOutcome::failure(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::AssertionFailed,
            format!(
                "DNS answers [{}] did not contain expected value '{}'",
                answers.join(", "),
                monitor.dns_expected_value.as_deref().unwrap_or_default()
            ),
        )),
        Err(err) => Ok(DnsCheckOutcome::failure(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::InternalError,
            err,
        )),
    }
}

/// Pure evaluation of DNS answers against an expected record value.
/// Returns `Ok(true)` when the check passes, `Ok(false)` when the
/// expected value is set and not present, and `Err` for malformed input.
fn evaluate_dns_answers(
    answers: &[String],
    expected: Option<&str>,
    host: &str,
    record_type: &str,
) -> std::result::Result<bool, String> {
    if answers.is_empty() {
        return Ok(false);
    }

    if let Some(expected) = expected
        && !answers.iter().any(|answer| answer == expected)
    {
        return Ok(false);
    }

    let _ = (host, record_type);
    Ok(true)
}

/// Backwards-compatible DNS entry point used by the legacy sweep
/// scheduler (`scheduler.rs`).
pub async fn check_dns(monitor: &Monitor) -> Result {
    let outcome = check_dns_outcome(monitor).await?;
    Ok(outcome.result)
}

fn rdata_to_string(rdata: &RData) -> String {
    match rdata {
        RData::A(ip) => ip.to_string(),
        RData::AAAA(ip) => ip.to_string(),
        RData::CNAME(name) => name.to_string(),
        RData::TXT(txt) => txt.to_string(),
        RData::MX(mx) => mx.exchange.to_string(),
        RData::NS(ns) => ns.to_string(),
        other => other.record_type().to_string(),
    }
}

fn parse_record_type(value: &str) -> std::result::Result<RecordType, String> {
    match value.to_ascii_uppercase().as_str() {
        "A" => Ok(RecordType::A),
        "AAAA" => Ok(RecordType::AAAA),
        "CNAME" => Ok(RecordType::CNAME),
        "TXT" => Ok(RecordType::TXT),
        "MX" => Ok(RecordType::MX),
        "NS" => Ok(RecordType::NS),
        other => Err(format!("unsupported DNS record type: {other}")),
    }
}

/// Builds a resolver using a hardcoded public resolver (Cloudflare), never
/// the system's `/etc/resolv.conf`. See the module-level SECURITY note.
fn build_public_resolver()
-> std::result::Result<TokioResolver, Box<dyn std::error::Error + Send + Sync>> {
    let builder = Resolver::builder_with_config(
        ResolverConfig::udp_and_tcp(&CLOUDFLARE),
        TokioRuntimeProvider::default(),
    );

    let resolver = builder
        .build()
        .map_err(|err| format!("failed to build DNS resolver: {err}"))?;

    Ok(resolver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_record_types() {
        assert_eq!(parse_record_type("a").unwrap(), RecordType::A);
        assert_eq!(parse_record_type("TXT").unwrap(), RecordType::TXT);
        assert_eq!(parse_record_type("cname").unwrap(), RecordType::CNAME);
    }

    #[test]
    fn rejects_unknown_record_type() {
        assert!(parse_record_type("SRV").is_err());
    }

    #[test]
    fn expected_value_match_succeeds() {
        let answers = vec!["93.184.216.34".to_string()];
        assert_eq!(
            evaluate_dns_answers(&answers, Some("93.184.216.34"), "example.com", "A"),
            Ok(true)
        );
    }

    #[test]
    fn expected_value_mismatch_fails() {
        let answers = vec!["93.184.216.34".to_string()];
        assert_eq!(
            evaluate_dns_answers(&answers, Some("1.2.3.4"), "example.com", "A"),
            Ok(false)
        );
    }

    #[test]
    fn missing_expected_value_but_answers_present_succeeds() {
        let answers = vec!["93.184.216.34".to_string()];
        assert_eq!(
            evaluate_dns_answers(&answers, None, "example.com", "A"),
            Ok(true)
        );
    }

    #[test]
    fn empty_answers_fails() {
        let answers: Vec<String> = vec![];
        assert_eq!(
            evaluate_dns_answers(&answers, None, "example.com", "A"),
            Ok(false)
        );
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/flow_executor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/flow_executor.rs")"
  cp "crates/worker/src/flow_executor.rs" "$BACKUP_DIR/crates/worker/src/flow_executor.rs"
fi

mkdir -p "$(dirname "crates/worker/src/flow_executor.rs")"
cat > "crates/worker/src/flow_executor.rs" <<'PINGVERTEX_EOF'
//! Gudanar da "API Flow" — jerin steps na HTTP request da ake gudanarwa
//! ɗaya-bayan-ɗaya, inda kowanne step zai iya amfani da sakamakon steps
//! da suka gabata (ta hanyar `{{steps.N....}}` placeholders).
//!
//! SECURITY: kowanne step, KODA YAUSHE, dole ya wuce ta cikin SSRF guard
//! (`crate::ssrf::resolve_and_validate_destination`) KAFIN a aika HTTP
//! request — koda url_template ɗin static ce, koda kuma ta samu wani
//! ɓangare daga extracted variable na wani step na baya (domin shi ne
//! ainihin abinda zai iya ɗauke da wani internal URL, misali idan wata
//! external API ta dawo da redirect zuwa `http://169.254.169.254/`).
//! Babu wani "fast path" da ke tsallake wannan.
//!
//! B29: `execute_flow` (the production entry point) always uses the real
//! SSRF resolver. `execute_flow_with_resolver` is a generalized version
//! that accepts an injected `DestinationResolverFn`, used ONLY by this
//! crate's own tests to pin steps to a local mock HTTP server without
//! weakening the production path — the exact same test-seam pattern
//! `checker.rs` already established with `check_http_with_validator`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};
use crate::placeholder_resolver::{StepResult, resolve_template};

/// Iyaka akan JIMLAR lokacin da flow guda ɗaya (mai steps har zuwa 10) zai
/// iya ɗauka KAFIN a soke shi baki ɗaya.
///
/// Wannan iyaka ce akan FLOW DUKA, BA kowanne step ɗaya-bayan-ɗaya ba —
/// idan kowanne step yana da nasa timeout mai zaman kansa kaɗai, flow mai
/// steps 10 na iya kwana yana gudana (misali idan kowanne step ya kusa kai
/// iyakarsa) tare da toshe worker resources ba tare da wani dalili na
/// gaskiya ba.
pub const FLOW_TOTAL_TIMEOUT_SECS: u64 = 60;

/// Bound on a single step's response body, mirroring
/// `checker::MAX_HTTP_RESPONSE_BODY_BYTES`. A flow step must never read an
/// unbounded response body into memory. Exceeding this cap fails the step
/// (and therefore, via fail-fast, the whole flow) rather than silently
/// truncating, so assertions/extracted variables never operate on partial
/// data.
const MAX_STEP_RESPONSE_BODY_BYTES: usize = 1024 * 1024;

/// Alias for a boxed future that is `Send` and borrows data with lifetime
/// `'a`. This avoids the malformed multi-line `Pin<Box<dyn Future…>>`
/// return-type syntax that previously broke this file.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Signature for a resolved SSRF-safe destination lookup, generalized so
/// tests can inject a fake resolver. See module docs.
pub type DestinationResolverFn = dyn for<'a> Fn(
        &'a str,
    ) -> BoxFuture<'a, std::result::Result<shared::ssrf::ResolvedDestination, String>>
    + Send
    + Sync;

fn real_ssrf_resolver(
    url: &str,
) -> BoxFuture<'_, std::result::Result<shared::ssrf::ResolvedDestination, String>> {
    Box::pin(crate::ssrf::resolve_and_validate_destination(url))
}

#[derive(Debug)]
pub struct FlowStepOutcome {
    pub step_order: u16,
    pub step_name: String,
    pub passed: bool,
    pub attempted: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub assertion_results: Vec<domain::value_objects::AssertionEvaluationResult>,
}

#[derive(Debug)]
pub struct FlowExecutionResult {
    pub flow_id: domain::shared::ids::ApiFlowId,
    pub passed: bool,
    pub failed_at_step: Option<u16>,
    pub outcomes: Vec<FlowStepOutcome>,
    pub total_duration_ms: u128,
}

/// Gudanar da DUK steps ɗin `flow`, ta amfani da REAL SSRF resolver ɗin
/// production (`crate::ssrf::resolve_and_validate_destination`). Wannan
/// shi ne KAWAI entry point da wani kira na waje (misali
/// `FlowCheckExecutor`) ya kamata ya yi amfani da shi.
pub async fn execute_flow(
    flow: &domain::entities::ApiFlow,
    http_client: &reqwest::Client,
) -> FlowExecutionResult {
    execute_flow_with_resolver(flow, http_client, &real_ssrf_resolver).await
}

/// Gudanar da DUK steps ɗin `flow`, ɗaya-bayan-ɗaya, FAIL-FAST akan step na
/// farko da ya kasa (ko dai domin placeholder resolution ya kasa, SSRF
/// guard ya toshe destination, HTTP request ɗin ya kasa, ko wani assertion
/// ya gaza) — babu wani step da zai gudana bayan wani step ya kasa, domin
/// steps na gaba na iya dogaro da "stale/empty variables" daga step da ya
/// gagara.
///
/// `resolver` is injectable ONLY so tests can pin steps to a local mock
/// server (see module docs). `execute_flow` above is the production entry
/// point and always passes the real SSRF resolver.
pub async fn execute_flow_with_resolver(
    flow: &domain::entities::ApiFlow,
    http_client: &reqwest::Client,
    resolver: &DestinationResolverFn,
) -> FlowExecutionResult {
    // `http_client` ba a amfani da shi kai-tsaye a nan: bin salon
    // `checker.rs::check_monitor` A-DAIDAI (wanda shi ma yana da
    // parameter `_client` amma yana gina SABON `reqwest::Client` tare da
    // `.resolve(host, socket_addr)` pinning na kansa daga `destination`
    // ɗin da SSRF guard ya riga ya tabbatar) — kowanne step yana gina
    // nasa pinned client bayan SSRF-resolve domin hana wani sabon DNS
    // lookup bayan an riga an tabbatar da adireshin. An bar wannan
    // parameter domin daidaita sigar aikin da sauran ayyukan wannan crate.
    let _ = http_client;

    let started = Instant::now();

    // An raba wannan da `run_all_steps` domin idan `tokio::time::timeout`
    // ya wuce lokaci ya soke (drop) future ɗin da ke gudanar da steps,
    // mun riga mun samu ci gaban da aka yi zuwa yanzu (outcomes na steps
    // da suka riga sun gama) maimakon ɓatar da su gaba ɗaya.
    let outcomes_so_far: Arc<Mutex<Vec<FlowStepOutcome>>> = Arc::new(Mutex::new(Vec::new()));
    // Order na step ɗin da ke "in-flight" (0 = babu). Ana sabunta wannan
    // KAFIN a fara aiwatar da kowanne step, don haka idan timeout ya kama
    // mu a tsakiyar wani step, mun san wanne ne.
    let in_flight_step: Arc<AtomicU16> = Arc::new(AtomicU16::new(0));
    let in_flight_attempted: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    let run = run_all_steps(
        flow,
        resolver,
        Arc::clone(&outcomes_so_far),
        Arc::clone(&in_flight_step),
        Arc::clone(&in_flight_attempted),
    );

    match tokio::time::timeout(Duration::from_secs(FLOW_TOTAL_TIMEOUT_SECS), run).await {
        Ok(()) => {
            let outcomes: Vec<FlowStepOutcome> = {
                let mut guard = outcomes_so_far.lock().await;
                guard.drain(..).collect()
            };

            let failed_at_step = outcomes.iter().find(|o| !o.passed).map(|o| o.step_order);
            let passed = failed_at_step.is_none();

            FlowExecutionResult {
                flow_id: flow.id,
                passed,
                failed_at_step,
                outcomes,
                total_duration_ms: started.elapsed().as_millis(),
            }
        }
        Err(_elapsed) => {
            // Flow ya wuce iyakar lokaci gabaɗaya. Mun dogara akan
            // `outcomes_so_far` (wanda aka sabunta bayan KOWANNE step ya
            // gama, ba a ƙarshen flow ba kaɗai) domin mu gina saƙon ƙarshe
            // mai amfani maimakon ɓata dukkan ci gaban da aka riga aka samu.
            let mut outcomes: Vec<FlowStepOutcome> = {
                let mut guard = outcomes_so_far.lock().await;
                guard.drain(..).collect()
            };

            let stalled_step = in_flight_step.load(Ordering::SeqCst);
            let stalled_name = flow
                .steps()
                .iter()
                .find(|s| s.order == stalled_step)
                .map(|s| s.name.as_str().to_string())
                .unwrap_or_default();

            outcomes.push(FlowStepOutcome {
                step_order: stalled_step,
                step_name: stalled_name,
                passed: false,
                attempted: in_flight_attempted.load(Ordering::Acquire),
                status_code: None,
                error: Some("flow exceeded 60s total timeout".to_string()),
                assertion_results: Vec::new(),
            });

            FlowExecutionResult {
                flow_id: flow.id,
                passed: false,
                failed_at_step: None,
                outcomes,
                total_duration_ms: started.elapsed().as_millis(),
            }
        }
    }
}

/// Aiwatar da DUK steps na `flow` ɗaya-bayan-ɗaya (ORDER ASC, kamar yadda
/// `flow.steps()` ya dawo da su), FAIL-FAST akan step na farko mai gaza.
///
/// Ana kiran wannan a CIKIN `tokio::time::timeout` daga `execute_flow_with_resolver` —
/// don haka wannan aikin BAI KAMATA ya yi nasa timeout na jimlar lokaci
/// ba, domin `execute_flow_with_resolver` ke da alhakin hakan. Kowanne outcome ana
/// turashi zuwa `outcomes_so_far` nan take bayan an gama shi, domin idan
/// aka soke wannan future daga waje (timeout), ci gaban da aka riga aka
/// samu bai ɓace ba.
async fn run_all_steps(
    flow: &domain::entities::ApiFlow,
    resolver: &DestinationResolverFn,
    outcomes_so_far: Arc<Mutex<Vec<FlowStepOutcome>>>,
    in_flight_step: Arc<AtomicU16>,
    in_flight_attempted: Arc<AtomicBool>,
) {
    let mut results: HashMap<u16, StepResult> = HashMap::new();

    for step in flow.steps() {
        in_flight_step.store(step.order, Ordering::SeqCst);
        in_flight_attempted.store(false, Ordering::Release);

        let (outcome, step_result) = execute_step(step, &results, resolver, Arc::clone(&in_flight_attempted)).await;

        if let Some(step_result) = step_result {
            results.insert(step.order, step_result);
        }

        let failed = !outcome.passed;

        {
            let mut guard = outcomes_so_far.lock().await;
            guard.push(outcome);
        }

        if failed {
            // FAIL-FAST: TSAYA nan take, KADA a ci gaba da steps na gaba.
            break;
        }
    }

    in_flight_step.store(0, Ordering::SeqCst);
    in_flight_attempted.store(false, Ordering::Release);
}

/// Aiwatar da step guda ɗaya kaɗai: resolve placeholders -> SSRF guard ->
/// aika HTTP request -> tantance assertions. Mayar da `(outcome, StepResult
/// idan an sami nasarar HTTP response)` — `StepResult` ɗin ana amfani da
/// shi domin steps na gaba su iya amfani da sakamakon wannan step (koda
/// idan wannan step ɗin ya kasa a ƙarshe saboda assertion, ba ruwan mu,
/// domin loop ɗin zai TSAYA nan take saboda FAIL-FAST kafin wani step na
/// gaba ya sami dama ya amfana da shi).
async fn execute_step(
    step: &domain::entities::ApiFlowStep,
    results: &HashMap<u16, StepResult>,
    resolver: &DestinationResolverFn,
    in_flight_attempted: Arc<AtomicBool>,
) -> (FlowStepOutcome, Option<StepResult>) {
    let step_name = step.name.as_str().to_string();

    let failed_outcome = |error: String| FlowStepOutcome {
        step_order: step.order,
        step_name: step_name.clone(),
        passed: false,
        attempted: false,
        status_code: None,
        error: Some(error),
        assertion_results: Vec::new(),
    };

    // a. Resolve url_template, kowanne header value, da body — KAWAI ta
    // hanyar `placeholder_resolver::resolve_template`, babu wata sabuwar
    // hanyar interpolation a nan.
    let resolved_url = match resolve_template(&step.url_template, results, step.order) {
        Ok(u) => u,
        Err(err) => return (failed_outcome(err), None),
    };

    let mut resolved_headers: Vec<(String, String)> = Vec::with_capacity(step.headers.len());
    for (key, value_template) in &step.headers {
        match resolve_template(value_template, results, step.order) {
            Ok(v) => resolved_headers.push((key.clone(), v)),
            Err(err) => return (failed_outcome(err), None),
        }
    }

    let resolved_body: Option<String> = match &step.body {
        Some(body_template) => match resolve_template(body_template, results, step.order) {
            Ok(b) => Some(b),
            Err(err) => return (failed_outcome(err), None),
        },
        None => None,
    };

    // b. SSRF GUARD — DOLE, BA TARE DA TOGIYA BA. Ana gudanar da wannan
    // KOYAUSHE, koda url_template ɗin static ce mai yiwuwa ta zama
    // internal address idan mai amfani ya rubuta ta da gangan, KUMA
    // musamman koda url_template ta samu wani ɓangare daga extracted
    // variable na wani step na baya.
    let destination = match resolver(&resolved_url).await {
        Ok(d) => d,
        Err(err) => return (failed_outcome(err), None),
    };

    let method = match step.method {
        domain::HttpMethod::Get => reqwest::Method::GET,
        domain::HttpMethod::Head => reqwest::Method::HEAD,
    };

    // c. Bin salon `checker.rs::check_monitor` A-DAIDAI: gina sabon client
    // mai `.resolve(host, socket_addr)` pinning daga `destination` da SSRF
    // guard ya riga ya tabbatar — babu sabon DNS lookup bayan an tabbatar
    // da adireshin, kuma redirect an kashe shi domin ba za mu bar wani
    // redirect ya zama sabuwar hanyar SSRF ba.
    let pinned_client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        // SECURITY: see the matching comment in `checker.rs` —
        // without `.no_proxy()`, a configured HTTP_PROXY/HTTPS_PROXY/
        // ALL_PROXY env var bypasses the `.resolve()` DNS pinning above,
        // reopening the SSRF/DNS-rebinding hole the destination
        // validation exists to close.
        .no_proxy()
        .build()
    {
        Ok(c) => c,
        Err(err) => return (failed_outcome(err.to_string()), None),
    };

    let mut request_builder = pinned_client.request(method, destination.url);
    for (key, value) in &resolved_headers {
        request_builder = request_builder.header(key, value);
    }
    if let Some(body) = &resolved_body {
        request_builder = request_builder.body(body.clone());
    }

    let started = Instant::now();
    in_flight_attempted.store(true, Ordering::Release);
    let response = request_builder.send().await;
    let latency = started.elapsed().as_millis();

    // d. Kasawar HTTP request (timeout, connection error, da sauransu).
    let response = match response {
        Ok(r) => r,
        Err(err) => return (failed_outcome(err.to_string()), None),
    };

    let status = response.status().as_u16();

    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    // Bounded body read — never reads more than
    // MAX_STEP_RESPONSE_BODY_BYTES into memory. Exceeding the cap fails
    // this step (and, via fail-fast, the whole flow) instead of silently
    // truncating.
    let body_bytes = match read_body_bounded(response).await {
        Ok(bytes) => bytes,
        Err(()) => {
            return (
                failed_outcome(
                    "HTTP response body exceeded the maximum permitted size".to_string(),
                ),
                None,
            );
        }
    };
    let body_string = String::from_utf8_lossy(&body_bytes).into_owned();

    // e. Ajiye StepResult domin steps na gaba su iya amfani dashi.
    let step_result = StepResult {
        status_code: status,
        headers: headers.clone(),
        body: body_string,
    };

    // f. Tantance assertions na wannan step. Koda step ɗin ya dawo da
    // HTTP 200 amma wani assertion ya kasa, wannan step din baki ɗayansa
    // YA KASA.
    let ctx = EvalContext {
        status_code: status,
        headers,
        body_bytes,
        latency_ms: latency,
    };

    let (all_passed, assertion_results) = evaluate_assertions(&step.assertions, &ctx);

    let error = if all_passed {
        None
    } else {
        let errors: Vec<String> = assertion_results
            .iter()
            .filter_map(|r| r.message.clone())
            .collect();
        Some(format!("assertions sun gaza: {}", errors.join("; ")))
    };

    // g.
    (
        FlowStepOutcome {
            step_order: step.order,
            step_name,
            passed: all_passed,
            status_code: Some(status),
            error,
            assertion_results,
        },
        Some(step_result),
    )
}

/// Reads a response body up to `MAX_STEP_RESPONSE_BODY_BYTES`. Never reads
/// unbounded and never accepts more than the configured cap. Mirrors
/// `checker::read_body_bounded`.
async fn read_body_bounded(mut response: reqwest::Response) -> std::result::Result<Vec<u8>, ()> {
    let mut body = Vec::new();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > MAX_STEP_RESPONSE_BODY_BYTES {
                    return Err(());
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => return Err(()),
        }
    }

    Ok(body)
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/http_check_executor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/http_check_executor.rs")"
  cp "crates/worker/src/http_check_executor.rs" "$BACKUP_DIR/crates/worker/src/http_check_executor.rs"
fi

mkdir -p "$(dirname "crates/worker/src/http_check_executor.rs")"
cat > "crates/worker/src/http_check_executor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc};

use application::ports::{CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind};
use tracing::{error, info};

use crate::check_error_mapping::map_check_error_kind_to_error_type;
use crate::checker::check_http;
use crate::incident_evaluator::IncidentEvaluator;
use crate::durable_scheduler::CheckExecution;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// B18 real HTTP/HTTPS executor for B17's `DurableScheduler`.
///
/// Executes a monitor only when `monitor.kind == MonitorKind::Http`.
/// Other monitor kinds are intentionally rejected so later check workers
/// (TCP/DNS/etc.) can be wired independently without this executor
/// silently treating them as success.
#[derive(Clone)]
pub struct HttpCheckExecutor {
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
}

impl HttpCheckExecutor {
    pub fn new(
        check_repository: Arc<dyn CheckRepository>,
        id_generator: Arc<dyn IdGenerator>,
        incident_evaluator: IncidentEvaluator,
    ) -> Self {
        Self {
            check_repository,
            id_generator,
            incident_evaluator,
        }
    }
}

#[async_trait]
impl crate::durable_scheduler::CheckExecutor for HttpCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Http {
            return Err(format!(
                "B18 HttpCheckExecutor only supports HTTP monitors, got {:?}",
                monitor.kind
            )
            .into());
        }

        let outcome = check_http(monitor).await;

        let status_code = outcome.result.status_code;
        let latency_ms = outcome.result.latency_ms;
        let error_message = outcome.result.error.clone();
        let cert_expires_at = outcome.result.cert_expires_at;
        let cert_days_remaining = outcome.result.cert_days_remaining;

        let check_id = self.id_generator.next_check_id().await;
        let status = if outcome.result.success {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        };

        let error_type = if outcome.result.success {
            ErrorType::None
        } else {
            map_check_error_kind_to_error_type(outcome.error_kind, status_code)
        };

        let check = Check::new(
            check_id,
            monitor.id,
            outcome.result.region.clone(),
            status,
            status_code,
            latency_ms,
            error_type,
            error_message,
        )
        .with_cert_info(cert_expires_at, cert_days_remaining);

        if let Err(err) = self.check_repository.save(&check).await {
            return Ok(CheckExecution::failed(outcome.attempted, format!("failed to persist check: {err}")));
        }

        info!(
            monitor_id = %monitor.id.0,
            monitor_name = %monitor.name.as_str(),
            check_id = %check.id.0,
            success = outcome.result.success,
            status_code = ?status_code,
            latency_ms = latency_ms,
            "HTTP check persisted"
        );

        // Non-fatal, mirroring the diagnostic report save above and the
        // notification-failure handling inside `IncidentManager`: a
        // failure to evaluate/persist incident state must never cause
        // the check itself (already persisted above) to be reported as
        // failed.
        if let Err(err) = self
            .incident_evaluator
            .evaluate(monitor, outcome.result.success)
            .await
        {
            error!(
                monitor_id = %monitor.id.0,
                error = %err,
                "failed to evaluate incident state"
            );
        }

        Ok(CheckExecution { attempted: outcome.attempted, error: None })
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/tcp_check_executor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/tcp_check_executor.rs")"
  cp "crates/worker/src/tcp_check_executor.rs" "$BACKUP_DIR/crates/worker/src/tcp_check_executor.rs"
fi

mkdir -p "$(dirname "crates/worker/src/tcp_check_executor.rs")"
cat > "crates/worker/src/tcp_check_executor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc};

use application::ports::{CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind};
use tracing::{error, info};

use crate::check_error_mapping::map_check_error_kind_to_error_type;
use crate::incident_evaluator::IncidentEvaluator;
use crate::durable_scheduler::CheckExecution;
use crate::tcp_checker::check_tcp_outcome;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// B19 TCP executor for B17's `DurableScheduler`. Persists a `Check`
/// using the same error taxonomy as the HTTP executor.
#[derive(Clone)]
pub struct TcpCheckExecutor {
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
}

impl TcpCheckExecutor {
    pub fn new(
        check_repository: Arc<dyn CheckRepository>,
        id_generator: Arc<dyn IdGenerator>,
        incident_evaluator: IncidentEvaluator,
    ) -> Self {
        Self {
            check_repository,
            id_generator,
            incident_evaluator,
        }
    }
}

#[async_trait]
impl crate::durable_scheduler::CheckExecutor for TcpCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Tcp {
            return Err(format!(
                "TcpCheckExecutor only supports TCP monitors, got {:?}",
                monitor.kind
            )
            .into());
        }

        let outcome = match check_tcp_outcome(monitor).await {
            Ok(outcome) => outcome,
            Err(err) => return Ok(CheckExecution::failed(false, err.to_string())),
        };

        let status_code = outcome.result.status_code;
        let latency_ms = outcome.result.latency_ms;
        let error_message = outcome.result.error.clone();

        let check_id = self.id_generator.next_check_id().await;
        let status = if outcome.result.success {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        };

        let error_type = if outcome.result.success {
            ErrorType::None
        } else {
            map_check_error_kind_to_error_type(outcome.error_kind, status_code)
        };

        let check = Check::new(
            check_id,
            monitor.id,
            outcome.result.region.clone(),
            status,
            status_code,
            latency_ms,
            error_type,
            error_message,
        );

        if let Err(err) = self.check_repository.save(&check).await {
            return Ok(CheckExecution::failed(outcome.attempted, format!("failed to persist check: {err}")));
        }

        info!(
            monitor_id = %monitor.id.0,
            monitor_name = %monitor.name.as_str(),
            check_id = %check.id.0,
            success = outcome.result.success,
            latency_ms = latency_ms,
            "TCP check persisted"
        );

        if let Err(err) = self
            .incident_evaluator
            .evaluate(monitor, outcome.result.success)
            .await
        {
            error!(
                monitor_id = %monitor.id.0,
                error = %err,
                "failed to evaluate incident state"
            );
        }

        Ok(CheckExecution { attempted: outcome.attempted, error: None })
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/dns_check_executor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/dns_check_executor.rs")"
  cp "crates/worker/src/dns_check_executor.rs" "$BACKUP_DIR/crates/worker/src/dns_check_executor.rs"
fi

mkdir -p "$(dirname "crates/worker/src/dns_check_executor.rs")"
cat > "crates/worker/src/dns_check_executor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc};

use application::ports::{CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind};
use tracing::{error, info};

use crate::check_error_mapping::map_check_error_kind_to_error_type;
use crate::dns_checker::check_dns_outcome;
use crate::incident_evaluator::IncidentEvaluator;
use crate::durable_scheduler::CheckExecution;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// B19 DNS executor for B17's `DurableScheduler`. Persists a `Check`
/// using the same error taxonomy as the HTTP executor.
#[derive(Clone)]
pub struct DnsCheckExecutor {
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
}

impl DnsCheckExecutor {
    pub fn new(
        check_repository: Arc<dyn CheckRepository>,
        id_generator: Arc<dyn IdGenerator>,
        incident_evaluator: IncidentEvaluator,
    ) -> Self {
        Self {
            check_repository,
            id_generator,
            incident_evaluator,
        }
    }
}

#[async_trait]
impl crate::durable_scheduler::CheckExecutor for DnsCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Dns {
            return Err(format!(
                "DnsCheckExecutor only supports DNS monitors, got {:?}",
                monitor.kind
            )
            .into());
        }

        let outcome = match check_dns_outcome(monitor).await {
            Ok(outcome) => outcome,
            Err(err) => return Ok(CheckExecution::failed(false, err.to_string())),
        };

        let status_code = outcome.result.status_code;
        let latency_ms = outcome.result.latency_ms;
        let error_message = outcome.result.error.clone();

        let check_id = self.id_generator.next_check_id().await;
        let status = if outcome.result.success {
            CheckStatus::Up
        } else {
            CheckStatus::Down
        };

        let error_type = if outcome.result.success {
            ErrorType::None
        } else {
            map_check_error_kind_to_error_type(outcome.error_kind, status_code)
        };

        let check = Check::new(
            check_id,
            monitor.id,
            outcome.result.region.clone(),
            status,
            status_code,
            latency_ms,
            error_type,
            error_message,
        );

        if let Err(err) = self.check_repository.save(&check).await {
            return Ok(CheckExecution::failed(outcome.attempted, format!("failed to persist check: {err}")));
        }

        info!(
            monitor_id = %monitor.id.0,
            monitor_name = %monitor.name.as_str(),
            check_id = %check.id.0,
            success = outcome.result.success,
            latency_ms = latency_ms,
            "DNS check persisted"
        );

        if let Err(err) = self
            .incident_evaluator
            .evaluate(monitor, outcome.result.success)
            .await
        {
            error!(
                monitor_id = %monitor.id.0,
                error = %err,
                "failed to evaluate incident state"
            );
        }

        Ok(CheckExecution { attempted: outcome.attempted, error: None })
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/flow_check_executor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/flow_check_executor.rs")"
  cp "crates/worker/src/flow_check_executor.rs" "$BACKUP_DIR/crates/worker/src/flow_check_executor.rs"
fi

mkdir -p "$(dirname "crates/worker/src/flow_check_executor.rs")"
cat > "crates/worker/src/flow_check_executor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc};

use application::ports::{ApiFlowRepository, CheckRepository, IdGenerator};
use async_trait::async_trait;
use domain::{Check, CheckStatus, ErrorType, Monitor, MonitorKind, RegionCode};
use tracing::{error, info};

use crate::flow_executor::{FlowExecutionResult, execute_flow};
use crate::incident_evaluator::IncidentEvaluator;
use crate::durable_scheduler::CheckExecution;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// B29 API Flow executor for B17's `DurableScheduler`. Executes a monitor
/// only when `monitor.kind == MonitorKind::Flow`, fetching the monitor's
/// `ApiFlow` and running every step via `flow_executor::execute_flow`
/// (SSRF-guarded independently per step, fail-fast, 60s total budget, 10
/// step cap — see `flow_executor.rs` and `domain::entities::ApiFlow`).
#[derive(Clone)]
pub struct FlowCheckExecutor {
    api_flow_repository: Arc<dyn ApiFlowRepository>,
    check_repository: Arc<dyn CheckRepository>,
    id_generator: Arc<dyn IdGenerator>,
    incident_evaluator: IncidentEvaluator,
    http_client: reqwest::Client,
}

impl FlowCheckExecutor {
    pub fn new(
        api_flow_repository: Arc<dyn ApiFlowRepository>,
        check_repository: Arc<dyn CheckRepository>,
        id_generator: Arc<dyn IdGenerator>,
        incident_evaluator: IncidentEvaluator,
    ) -> Self {
        Self {
            api_flow_repository,
            check_repository,
            id_generator,
            incident_evaluator,
            // `execute_flow`'s `http_client` parameter is unused (each step
            // pins its own SSRF-validated client — see flow_executor.rs);
            // kept only so this executor's shape matches Http/Tcp/Dns.
            http_client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("building a redirect-disabled reqwest client never fails"),
        }
    }
}

#[async_trait]
impl crate::durable_scheduler::CheckExecutor for FlowCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        if monitor.kind != MonitorKind::Flow {
            return Ok(CheckExecution::failed(false, format!(
                "FlowCheckExecutor received unsupported monitor kind {:?}", monitor.kind
            )));
        }

        let flow = match self.api_flow_repository.find_by_monitor_id(monitor.id).await {
            Ok(Some(flow)) => flow,
            Ok(None) => return Ok(CheckExecution::failed(false, format!(
                "monitor {} is kind=Flow but has no api_flow row", monitor.id.0
            ))),
            Err(err) => return Ok(CheckExecution::failed(false, format!(
                "failed to load flow: {err}"
            ))),
        };

        let result = execute_flow(&flow, &self.http_client).await;
        let attempted = result.outcomes.iter().any(|o| o.attempted);

        let check_id = self.id_generator.next_check_id().await;
        let check = build_check(check_id, monitor, &result);

        if let Err(err) = self.check_repository.save(&check).await {
            return Ok(CheckExecution::failed(attempted, format!(
                "failed to persist check: {err}"
            )));
        }

        info!(
            monitor_id = %monitor.id.0,
            monitor_name = %monitor.name.as_str(),
            check_id = %check.id.0,
            success = result.passed,
            failed_at_step = ?result.failed_at_step,
            latency_ms = result.total_duration_ms,
            "flow check persisted"
        );

        if let Err(err) = self.incident_evaluator.evaluate(monitor, result.passed).await {
            error!(monitor_id = %monitor.id.0, error = %err, "failed to evaluate incident state");
        }

        Ok(CheckExecution { attempted, error: None })
    }
}

fn build_check(
    check_id: domain::shared::ids::CheckId,
    monitor: &Monitor,
    result: &FlowExecutionResult,
) -> Check {
    let status = if result.passed {
        CheckStatus::Up
    } else {
        CheckStatus::Down
    };

    let failing_outcome = result.outcomes.iter().find(|o| !o.passed);

    let status_code = failing_outcome
        .and_then(|o| o.status_code)
        .or_else(|| result.outcomes.last().and_then(|o| o.status_code));

    let error_type = failing_outcome
        .and_then(|o| o.error.as_deref())
        .map(|err| classify_flow_error(err, status_code))
        .unwrap_or(ErrorType::None);

    let error_message = failing_outcome.map(|o| {
        format!(
            "step {} ({}): {}",
            o.step_order,
            o.step_name,
            o.error.clone().unwrap_or_default()
        )
    });

    Check::new(
        check_id,
        monitor.id,
        RegionCode::default_region(),
        status,
        status_code,
        result.total_duration_ms,
        error_type,
        error_message,
    )
}

/// Flow step failures carry a plain `String` error (see
/// `flow_executor::FlowStepOutcome`), not `checker.rs`'s `CheckErrorKind`
/// — a flow step can fail for reasons (placeholder resolution, SSRF
/// block) the single-request taxonomy doesn't model. This is a best-effort
/// text classification onto the persisted `ErrorType` column, matching
/// `check_error_mapping::map_check_error_kind_to_error_type`'s categories
/// as closely as the available information allows.
fn classify_flow_error(error: &str, status_code: Option<u16>) -> ErrorType {
    let lower = error.to_lowercase();

    if lower.contains("ssrf blocked")
        || lower.contains("failed to resolve")
        || lower.contains("did not resolve")
    {
        ErrorType::Dns
    } else if lower.contains("timeout") || lower.contains("timed out") {
        ErrorType::Timeout
    } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        ErrorType::Ssl
    } else if let Some(status) = status_code {
        if (400..500).contains(&status) {
            ErrorType::Http4xx
        } else if status >= 500 {
            ErrorType::Http5xx
        } else {
            ErrorType::Unknown
        }
    } else {
        ErrorType::Unknown
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/multi_check_executor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/multi_check_executor.rs")"
  cp "crates/worker/src/multi_check_executor.rs" "$BACKUP_DIR/crates/worker/src/multi_check_executor.rs"
fi

mkdir -p "$(dirname "crates/worker/src/multi_check_executor.rs")"
cat > "crates/worker/src/multi_check_executor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use domain::{Monitor, MonitorKind};

use crate::dns_check_executor::DnsCheckExecutor;
use crate::durable_scheduler::CheckExecutor;
use crate::flow_check_executor::FlowCheckExecutor;
use crate::http_check_executor::HttpCheckExecutor;
use crate::incident_evaluator::IncidentEvaluator;
use crate::tcp_check_executor::TcpCheckExecutor;
use crate::durable_scheduler::CheckExecution;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Dispatches an active check to the correct executor based on
/// `monitor.kind`. Heartbeat monitors are rejected here because they are
/// passive and are handled by the separate `HeartbeatSweeper` loop.
#[derive(Clone)]
pub struct MultiCheckExecutor {
    http: HttpCheckExecutor,
    tcp: TcpCheckExecutor,
    dns: DnsCheckExecutor,
    flow: FlowCheckExecutor,
}

impl MultiCheckExecutor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        check_repository: Arc<dyn application::ports::CheckRepository>,
        id_generator: Arc<dyn application::ports::IdGenerator>,
        incident_evaluator: IncidentEvaluator,
        api_flow_repository: Arc<dyn application::ports::ApiFlowRepository>,
    ) -> Self {
        Self {
            http: HttpCheckExecutor::new(
                check_repository.clone(),
                id_generator.clone(),
                incident_evaluator.clone(),
            ),
            tcp: TcpCheckExecutor::new(
                check_repository.clone(),
                id_generator.clone(),
                incident_evaluator.clone(),
            ),
            dns: DnsCheckExecutor::new(
                check_repository.clone(),
                id_generator.clone(),
                incident_evaluator.clone(),
            ),
            flow: FlowCheckExecutor::new(
                api_flow_repository,
                check_repository,
                id_generator,
                incident_evaluator,
            ),
        }
    }
}

#[async_trait]
impl CheckExecutor for MultiCheckExecutor {
    async fn execute(&self, monitor: &Monitor) -> Result<CheckExecution> {
        match monitor.kind {
            MonitorKind::Http => self.http.execute(monitor).await,
            MonitorKind::Tcp => self.tcp.execute(monitor).await,
            MonitorKind::Dns => self.dns.execute(monitor).await,
            MonitorKind::Flow => self.flow.execute(monitor).await,
            MonitorKind::Heartbeat => {
                tracing::warn!(
                    monitor_id = %monitor.id.0,
                    "heartbeat monitor reached active executor; this is a scheduling bug"
                );
                Ok(CheckExecution::failed(
                    false,
                    "heartbeat monitors must be handled by HeartbeatSweeper, not active executor",
                ))
            }
        }
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/proxy_checker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/proxy_checker.rs")"
  cp "crates/worker/src/proxy_checker.rs" "$BACKUP_DIR/crates/worker/src/proxy_checker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/proxy_checker.rs")"
cat > "crates/worker/src/proxy_checker.rs" <<'PINGVERTEX_EOF'
//! Worker-side execution of a `ProxyCheckJob`: checks every `ProxyEntry`
//! belonging to the job concurrently (bounded by a semaphore), persists
//! each entry's result as it finishes, then marks the job completed.
//!
//! SSRF defense: every entry is resolved/validated via
//! `crate::ssrf::resolve_and_validate_destination` (Layer 2 — DNS
//! resolution + private-IP rejection + address pinning) immediately
//! before connecting, exactly as `crate::checker::check_monitor` does for
//! monitor URLs. No new SSRF logic is implemented here; this module only
//! calls the existing shared validator.
//!
//! Protocol verification is manual and pinned to the validated
//! `SocketAddr`:
//! - HTTP proxies are verified with an absolute-form HTTP GET to
//!   `http://example.com/`.
//! - HTTPS proxies are first connected over TLS to the proxy, then
//!   verified with a CONNECT tunnel to `example.com:443` and a TLS
//!   handshake through that tunnel.
//! - SOCKS5 proxies are verified with a full SOCKS5 handshake to
//!   `example.com:443`, then a TLS handshake through the tunnel.
//!
//! `supports_https` is set only when a TLS handshake through the proxy to
//! `example.com:443` actually succeeds.
//!
//! Response size limits and timeouts bound all proxy-generated data.

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use application::ports::ProxyCheckJobRepository;
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use base64::Engine;
use domain::ProxyScheme;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::value_objects::{ProxyCredentials, ProxyResult};
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Semaphore,
    task::JoinSet,
    time::timeout,
};
use tracing::{error, info, warn};

/// Upper bound on how many proxy entries within a single job are checked
/// concurrently. Keeps one bulk job from monopolizing this worker
/// process's connection/file-descriptor budget while other monitors,
/// flows, and jobs belonging to other users are also being processed
/// concurrently in the same process.
const MAX_CONCURRENT_CHECKS_PER_JOB: usize = 20;

/// Per-entry timeout for the TCP connect attempt.
const PER_ENTRY_TIMEOUT_SECS: u64 = 10;

/// Per-protocol-step timeout (read/write/TLS handshake) within a proxy
/// check. A malicious/slow proxy must not stall a job indefinitely.
const PROTOCOL_TIMEOUT_SECS: u64 = 5;

/// Maximum number of bytes accepted from a proxy when reading HTTP
/// response/status headers. Anything larger is treated as a protocol
/// failure.
const MAX_PROXY_RESPONSE_HEADER_BYTES: usize = 8192;

/// Fixed public target used to verify HTTPS/CONNECT/SOCKS tunneling.
/// Deliberately constant, never user input.
const HTTPS_VERIFY_TARGET_HOST: &str = "example.com";
const HTTPS_VERIFY_TARGET_PORT: u16 = 443;

fn proxy_scheme_str(scheme: &ProxyScheme) -> &'static str {
    match scheme {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks5 => "socks5",
    }
}

fn elapsed_ms(started: Instant) -> u32 {
    started.elapsed().as_millis().try_into().unwrap_or(u32::MAX)
}

fn error_code(scheme: &ProxyScheme, message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("ssrf blocked") {
        return "PRIVATE_IP_BLOCKED";
    }
    if lower.contains("failed to resolve") || lower.contains("did not resolve") {
        return "DNS_FAILED";
    }
    if lower.contains("invalid monitor url")
        || lower.contains("has no host")
        || lower.contains("invalid port")
    {
        return "INVALID_PROXY";
    }
    if lower.contains("connection refused") {
        return "CONNECTION_REFUSED";
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return if lower.contains("target")
            || lower.contains("tls handshake") && lower.contains("target")
        {
            "TARGET_TIMEOUT"
        } else {
            "CONNECTION_TIMEOUT"
        };
    }
    if lower.contains("authentication failed") || lower.contains("auth") && lower.contains("failed")
    {
        return "AUTH_FAILED";
    }
    if lower.contains("tls") {
        return "TLS_FAILED";
    }
    match scheme {
        ProxyScheme::Http => "HTTP_PROXY_FAILED",
        ProxyScheme::Https => "HTTPS_PROXY_FAILED",
        ProxyScheme::Socks5 => "SOCKS5_FAILED",
    }
}

fn proxy_result(
    entry: &ProxyEntry,
    started: Instant,
    reachable: bool,
    supports_https: bool,
    error: Option<String>,
) -> ProxyResult {
    let mut safe_error = error;
    if let Some(message) = safe_error.as_mut() {
        message.truncate(512);
    }
    let error_code = safe_error
        .as_deref()
        .map(|e| error_code(entry.scheme(), e).to_string());
    ProxyResult {
        host: entry.host().as_str().to_string(),
        port: entry.port(),
        scheme: proxy_scheme_str(entry.scheme()).to_string(),
        reachable,
        latency_ms: Some(elapsed_ms(started)),
        supports_https,
        anonymity: None,
        country_code: None,
        error_code,
        error_message: safe_error,
    }
}

async fn write_all_timeout<S>(
    stream: &mut S,
    data: &[u8],
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    timeout(timeout_duration, stream.write_all(data))
        .await
        .map_err(|_| "timed out writing to proxy".to_string())?
        .map_err(|e| format!("failed to write to proxy: {e}"))?;

    Ok(())
}

async fn read_exact_timeout<S>(
    stream: &mut S,
    buf: &mut [u8],
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + Unpin,
{
    timeout(timeout_duration, stream.read_exact(buf))
        .await
        .map_err(|_| "timed out reading from proxy".to_string())?
        .map_err(|e| format!("failed to read from proxy: {e}"))?;

    Ok(())
}

async fn read_http_header<S>(stream: &mut S, timeout_duration: Duration) -> Result<String, String>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        let n = timeout(timeout_duration, stream.read(&mut chunk))
            .await
            .map_err(|_| "timed out waiting for proxy response".to_string())?
            .map_err(|e| format!("failed to read proxy response: {e}"))?;

        if n == 0 {
            return Err("proxy closed connection before completing response headers".to_string());
        }

        if buf.len().saturating_add(n) > MAX_PROXY_RESPONSE_HEADER_BYTES {
            return Err("proxy response headers exceeded size limit".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);

        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(buf).map_err(|_| "proxy response was not valid UTF-8".to_string())
}

fn parse_status_code(header: &str) -> Result<u16, String> {
    let first_line = header.lines().next().ok_or("empty proxy response")?;
    let mut parts = first_line.split_whitespace();

    let version = parts.next().ok_or("missing HTTP version")?;
    if !version.starts_with("HTTP/") {
        return Err(format!("invalid HTTP version '{version}'"));
    }

    let status = parts
        .next()
        .ok_or("missing HTTP status code")?
        .parse::<u16>()
        .map_err(|_| "invalid HTTP status code".to_string())?;

    Ok(status)
}

fn proxy_authorization_header(creds: Option<&ProxyCredentials>) -> String {
    match creds {
        Some(creds) => {
            let token = format!("{}:{}", creds.username(), creds.password());
            format!(
                "Proxy-Authorization: Basic {}\r\n",
                base64::engine::general_purpose::STANDARD.encode(token.as_bytes())
            )
        }
        None => String::new(),
    }
}

async fn verify_http_proxy_absolute_form<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "GET http://{HTTPS_VERIFY_TARGET_HOST}/ HTTP/1.1\r\nHost: {HTTPS_VERIFY_TARGET_HOST}\r\nUser-Agent: PingVertex/1.0\r\nConnection: close\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration).await?;
    let header = read_http_header(stream, timeout_duration).await?;
    let status = parse_status_code(&header)?;

    if status == 401 || status == 407 {
        return Err(format!(
            "proxy authentication failed with HTTP status {status}"
        ));
    }
    if !(200..400).contains(&status) {
        return Err(format!("proxy returned HTTP status {status}"));
    }

    Ok(())
}

async fn verify_http_connect_tunnel<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let target = format!("{HTTPS_VERIFY_TARGET_HOST}:{HTTPS_VERIFY_TARGET_PORT}");
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nUser-Agent: PingVertex/1.0\r\nProxy-Connection: keep-alive\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration).await?;
    let header = read_http_header(stream, timeout_duration).await?;
    let status = parse_status_code(&header)?;

    if status == 401 || status == 407 {
        return Err(format!(
            "proxy authentication failed with CONNECT status {status}"
        ));
    }
    if !(200..300).contains(&status) {
        return Err(format!("proxy returned CONNECT status {status}"));
    }

    Ok(())
}

async fn tls_handshake<S>(
    stream: S,
    hostname: &str,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let connector = crate::proxy_tls::tls_connector().map_err(|e| e.to_string())?;
    let server_name = ServerName::try_from(hostname.to_string())
        .map_err(|_| "invalid TLS SNI hostname".to_string())?;

    timeout(timeout_duration, connector.connect(server_name, stream))
        .await
        .map_err(|_| "timed out during TLS handshake".to_string())?
        .map_err(|e| format!("TLS handshake failed: {e}"))?;

    Ok(())
}

async fn verify_socks5_proxy<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    target_host: &str,
    target_port: u16,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let methods: &[u8] = if creds.is_some() {
        &[5, 1, 2]
    } else {
        &[5, 1, 0]
    };
    write_all_timeout(stream, methods, timeout_duration).await?;

    let mut method_resp = [0u8; 2];
    read_exact_timeout(stream, &mut method_resp, timeout_duration).await?;

    if method_resp[0] != 5 {
        return Err("SOCKS5 server returned invalid version".to_string());
    }
    if method_resp[1] == 0xFF {
        return Err("SOCKS5 server does not accept any authentication method".to_string());
    }

    match method_resp[1] {
        2 => {
            let creds = creds.ok_or_else(|| {
                "SOCKS5 server selected username/password auth but no credentials were provided"
                    .to_string()
            })?;
            let username = creds.username().as_bytes();
            let password = creds.password().as_bytes();

            if username.len() > 255 || password.len() > 255 {
                return Err("SOCKS5 credentials too long".to_string());
            }

            let mut auth_req = Vec::with_capacity(3 + username.len() + password.len());
            auth_req.push(1);
            auth_req.push(username.len() as u8);
            auth_req.extend_from_slice(username);
            auth_req.push(password.len() as u8);
            auth_req.extend_from_slice(password);

            write_all_timeout(stream, &auth_req, timeout_duration).await?;

            let mut auth_resp = [0u8; 2];
            read_exact_timeout(stream, &mut auth_resp, timeout_duration).await?;

            if auth_resp[0] != 1 || auth_resp[1] != 0 {
                return Err("SOCKS5 username/password authentication failed".to_string());
            }
        }
        0 => {}
        other => {
            return Err(format!(
                "SOCKS5 server selected unsupported authentication method {other}"
            ));
        }
    }

    let host_bytes = target_host.as_bytes();
    if host_bytes.len() > 255 {
        return Err("target host too long for SOCKS5".to_string());
    }

    let mut connect_req = Vec::with_capacity(7 + host_bytes.len());
    connect_req.push(5);
    connect_req.push(1);
    connect_req.push(0);
    connect_req.push(3);
    connect_req.push(host_bytes.len() as u8);
    connect_req.extend_from_slice(host_bytes);
    connect_req.extend_from_slice(&target_port.to_be_bytes());

    write_all_timeout(stream, &connect_req, timeout_duration).await?;

    let mut connect_head = [0u8; 4];
    read_exact_timeout(stream, &mut connect_head, timeout_duration).await?;

    if connect_head[0] != 5 {
        return Err("SOCKS5 connect reply had invalid version".to_string());
    }
    if connect_head[1] != 0 {
        return Err(format!(
            "SOCKS5 connect failed with reply code {}",
            connect_head[1]
        ));
    }

    match connect_head[3] {
        1 => {
            let mut rest = [0u8; 6];
            read_exact_timeout(stream, &mut rest, timeout_duration).await?;
        }
        4 => {
            let mut rest = [0u8; 18];
            read_exact_timeout(stream, &mut rest, timeout_duration).await?;
        }
        3 => {
            let mut len_buf = [0u8; 1];
            read_exact_timeout(stream, &mut len_buf, timeout_duration).await?;
            let len = len_buf[0] as usize;
            let mut addr = vec![0u8; len];
            read_exact_timeout(stream, &mut addr, timeout_duration).await?;
            let mut port_buf = [0u8; 2];
            read_exact_timeout(stream, &mut port_buf, timeout_duration).await?;
        }
        _ => return Err("SOCKS5 connect reply had invalid address type".to_string()),
    }

    Ok(())
}

/// Checks a single proxy entry and returns its `ProxyResult`.
///
/// Steps:
/// 1. Build a `scheme://host:port` URL from the entry's own fields and
///    run it through `resolve_and_validate_destination` — this performs
///    the DNS lookup exactly once, rejects any private/reserved resolved
///    address (Layer 2 SSRF defense, on top of `ProxyHost::new`'s
///    literal-IP check already performed at submission time), and
///    returns the pinned `SocketAddr` to connect to.
/// 2. Connect to that pinned address and perform the protocol-specific
///    verification described in the module docs.
///
/// A resolution/validation failure and a connection failure are both
/// reported as `reachable: false` with a descriptive `error`, never as a
/// panic or a propagated `Result` — one bad proxy entry must never stop
/// the rest of the job from being checked.
async fn check_single_entry(entry: &ProxyEntry) -> (ProxyResult, bool) {
    let target = format!(
        "{}://{}:{}",
        proxy_scheme_str(entry.scheme()),
        entry.host().as_str(),
        entry.port()
    );

    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_public_destination(&target).await {
        Ok(destination) => destination,
        Err(err) => {
            let result = proxy_result(entry, started, false, false, Some(err.clone()));
            let code = result.error_code.as_deref().unwrap_or_default();
            let attempted = code != "PRIVATE_IP_BLOCKED" && code != "INVALID_PROXY";
            return (result, attempted);
        }
    };

    let connect_timeout = Duration::from_secs(PER_ENTRY_TIMEOUT_SECS);
    let protocol_timeout = Duration::from_secs(PROTOCOL_TIMEOUT_SECS);

    let (reachable, supports_https, error) = match entry.scheme() {
        ProxyScheme::Http => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(mut stream)) => {
                    match verify_http_proxy_absolute_form(
                        &mut stream,
                        entry.credentials(),
                        protocol_timeout,
                    )
                    .await
                    {
                        Err(err) => (false, false, Some(err)),
                        Ok(()) => {
                            match timeout(
                                connect_timeout,
                                TcpStream::connect(destination.socket_addr),
                            )
                            .await
                            {
                                Ok(Ok(mut tunnel_stream)) => {
                                    match verify_http_connect_tunnel(
                                        &mut tunnel_stream,
                                        entry.credentials(),
                                        protocol_timeout,
                                    )
                                    .await
                                    {
                                        Ok(()) => match tls_handshake(
                                            tunnel_stream,
                                            HTTPS_VERIFY_TARGET_HOST,
                                            protocol_timeout,
                                        )
                                        .await
                                        {
                                            Ok(()) => (true, true, None),
                                            Err(err) => (
                                                true,
                                                false,
                                                Some(format!("HTTPS target TLS failed: {err}")),
                                            ),
                                        },
                                        Err(err) => (
                                            true,
                                            false,
                                            Some(format!("HTTPS tunnel failed: {err}")),
                                        ),
                                    }
                                }
                                Ok(Err(err)) => (
                                    true,
                                    false,
                                    Some(format!("HTTPS tunnel connection failed: {err}")),
                                ),
                                Err(_) => (
                                    true,
                                    false,
                                    Some("HTTPS tunnel connection timed out".to_string()),
                                ),
                            }
                        }
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
        ProxyScheme::Https => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(tcp)) => {
                    let connector = match crate::proxy_tls::tls_connector() {
                        Ok(c) => c,
                        Err(err) => {
                            return (proxy_result(
                                entry, started, false, false, Some(err.to_string()),
                            ), true);
                        }
                    };
                    let server_name = match ServerName::try_from(entry.host().as_str().to_string())
                    {
                        Ok(name) => name,
                        Err(err) => {
                            return (proxy_result(
                                entry, started, false, false, Some(err.to_string()),
                            ), true);
                        }
                    };

                    match timeout(protocol_timeout, connector.connect(server_name, tcp)).await {
                        Ok(Ok(mut proxy_tls)) => {
                            if let Err(err) = verify_http_connect_tunnel(
                                &mut proxy_tls,
                                entry.credentials(),
                                protocol_timeout,
                            )
                            .await
                            {
                                (false, false, Some(err))
                            } else if let Err(err) =
                                tls_handshake(proxy_tls, HTTPS_VERIFY_TARGET_HOST, protocol_timeout)
                                    .await
                            {
                                (false, false, Some(err))
                            } else {
                                (true, true, None)
                            }
                        }
                        Ok(Err(err)) => (false, false, Some(format!("TLS to proxy failed: {err}"))),
                        Err(_) => (
                            false,
                            false,
                            Some("TLS handshake with proxy timed out".to_string()),
                        ),
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
        ProxyScheme::Socks5 => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(mut stream)) => {
                    if let Err(err) = verify_socks5_proxy(
                        &mut stream,
                        entry.credentials(),
                        HTTPS_VERIFY_TARGET_HOST,
                        HTTPS_VERIFY_TARGET_PORT,
                        protocol_timeout,
                    )
                    .await
                    {
                        (false, false, Some(err))
                    } else if let Err(err) =
                        tls_handshake(stream, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await
                    {
                        (
                            true,
                            false,
                            Some(format!(
                                "SOCKS5 tunnel connected but TLS to target failed: {err}"
                            )),
                        )
                    } else {
                        (true, true, None)
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
    };

    let result = proxy_result(entry, started, reachable, supports_https, error);
    (result, true)
}

/// Runs every entry of `job` through `check_single_entry`, persisting
/// each result and the job's overall progress/status as it goes.
///
/// `job` must already have been persisted (via
/// `ProxyCheckJobRepository::save_job`, called by the application layer's
/// submit use case) and `entries` must be exactly the entries belonging
/// to it. This function's only responsibilities are: transition the job
/// to `Running` (an optimistic-lock no-op if some other worker already
/// picked it up — see `ProxyCheckJob::mark_running`'s doc comment), check
/// every entry concurrently (bounded by `MAX_CONCURRENT_CHECKS_PER_JOB`),
/// persist each entry's result via `update_entry`, and finally mark the
/// job `Completed` and persist that.
///
/// Dispatch (how a submitted job reaches this function in the first
/// place — polling, LISTEN/NOTIFY, a channel, etc.) is out of scope here
/// and left to a separate prompt, per the task boundary for this module.
pub async fn run_proxy_check_job<R>(
    repository: &R,
    mut job: ProxyCheckJob,
    entries: Vec<ProxyEntry>,
    worker_id: &str,
    metering: Option<Arc<MeterUsageUseCase>>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: ProxyCheckJobRepository + ?Sized,
{
    // The worker already performed the atomic pending -> running claim.
    // The executor MUST NOT own or repeat that state transition.
    if job.status() != domain::ProxyCheckJobStatus::Running {
        info!(job_id = %job.id(), "proxy check job is not running; skipping");
        return Ok(());
    }

    if entries.len() != job.total_count() as usize {
        let mut failed = job.clone();
        if failed.mark_failed(format!(
            "proxy entry count mismatch: expected {}, loaded {}",
            job.total_count(),
            entries.len()
        )) {
            repository.update_job(&failed).await?;
        }
        return Ok(());
    }

    let persisted_successes = entries.iter().filter(|e| e.result().is_some_and(|r| r.reachable)).count() as u32;
    let persisted_failures = entries.iter().filter(|e| e.result().is_some_and(|r| !r.reachable)).count() as u32;
    job.rebuild_progress(persisted_successes, persisted_failures);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS_PER_JOB));
    let mut tasks = JoinSet::new();
    for entry in entries {
        let had_existing_result = entry.result().is_some();
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore remains open");
            let mut entry = entry;
            let entry_id = entry.id();
            let job_id = job.id();
            let (result, billing_blocked) = if let Some(metering) = metering.clone() {
                let request_id = format!("proxy-check:{}:{}", job_id, entry_id);
                if had_existing_result && metering.find_by_request(job.tenant_id().0, &request_id).await?.is_some() {
                    (entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("PAYG execution was already finalized".to_owned()))), false)
                } else {
                match metering.execute_classified_attempt_with_output(
                    MeterUsageInput {
                        tenant_id: job.tenant_id().0,
                        operation: "proxy_check".to_owned(),
                        units: 1,
                        request_id,
                    },
                    || async {
                        if had_existing_result {
                            let result = entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("persisted proxy result missing".to_owned())));
                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted: true, error: None }, result))
                        } else {
                            let (result, attempted) = check_single_entry(&entry).await;
                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted, error: None }, result))
                        }
                    },
                ).await {
                    Ok((_, Some(result))) => (result, false),
                    Ok((_, None)) => {
                        (entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("PAYG execution was already finalized".to_owned()))), false)
                    }
                    Err(application::ChargeForUsageError::InsufficientBalance) => {
                        let result = entry.result().cloned().unwrap_or_else(|| proxy_result(
                            &entry, Instant::now(), false, false,
                            Some("PAYG balance exhausted; top up the wallet to resume proxy checks".to_owned()),
                        ));
                        (result, true)
                    }
                    Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => {
                        let result = proxy_result(&entry, Instant::now(), false, false, Some(reason));
                        (result, false)
                    }
                    Err(err) => return Err(Box::new(err) as Box<dyn Error + Send + Sync>),
                }
                }
            } else {
                let (result, _) = check_single_entry(&entry).await;
                (result, false)
            };
            entry.set_result(result);
            (entry, !had_existing_result, billing_blocked)
        });
    }

    let mut fatal_reason: Option<String> = None;
    let heartbeat_every = Duration::from_secs(15);

    loop {
        match timeout(heartbeat_every, tasks.join_next()).await {
            Ok(Some(Ok((entry, should_count, entry_billing_blocked)))) => {
                billing_blocked |= entry_billing_blocked;
                if let Err(err) = repository.update_entry(&entry, job.tenant_id()).await {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to persist proxy entry result: {err}")
                    });
                    error!(job_id=%job.id(), entry_id=%entry.id(), error=%err, "proxy result persistence failed");
                    continue;
                }

                if should_count {
                    job.record_result(entry.result().is_some_and(|r| r.reachable));
                }

                // Persist progress periodically rather than once per proxy;
                // the entry result itself is already durably stored. This keeps
                // bulk jobs cheap while the separate heartbeat preserves the lease.
                if (job.checked_count() == job.total_count()
                    || job.checked_count().is_multiple_of(10))
                    && let Err(err) = repository.update_job(&job).await
                {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to persist proxy check job progress: {err}")
                    });
                    error!(
                        job_id=%job.id(),
                        error=%err,
                        "proxy job progress persistence failed"
                    );
                }

                // Lease ownership is separate from job progress so a failed
                // progress write cannot accidentally hide the heartbeat.
                match repository.heartbeat(job.id(), worker_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        fatal_reason.get_or_insert_with(|| "proxy job lease was lost".to_string());
                        break;
                    }
                    Err(err) => {
                        fatal_reason.get_or_insert_with(|| {
                            format!("failed to heartbeat proxy check job: {err}")
                        });
                        error!(job_id=%job.id(), error=%err, "proxy job heartbeat failed");
                        break;
                    }
                }
            }
            Ok(Some(Err(join_err))) => {
                fatal_reason.get_or_insert_with(|| format!("proxy entry task failed: {join_err}"));
                warn!(job_id=%job.id(), error=%join_err, "proxy entry task failed");
            }
            Ok(None) => break,
            Err(_) => match repository.heartbeat(job.id(), worker_id).await {
                Ok(true) => {}
                Ok(false) => {
                    fatal_reason.get_or_insert_with(|| "proxy job lease was lost".to_string());
                    break;
                }
                Err(err) => {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to heartbeat proxy check job: {err}")
                    });
                    break;
                }
            },
        }
    }

    if billing_blocked {
        // Keep the durable job Running when one or more entries were blocked by
        // insufficient PAYG balance. The lease will expire, stale-job recovery
        // will return it to Pending, and a later sweep can retry after top-up.
        repository.update_job(&job).await?;
        return Ok(());
    }

    if let Some(reason) = fatal_reason {
        if job.status() == domain::ProxyCheckJobStatus::Running && job.mark_failed(reason) {
            repository.update_job(&job).await?;
        }
        return Ok(());
    }

    if job.checked_count() != job.total_count()
        || job.checked_count() != job.successful_count().saturating_add(job.failed_count())
    {
        if job.mark_failed("proxy result persistence incomplete") {
            repository.update_job(&job).await?;
        }
        return Ok(());
    }

    if job.mark_completed() {
        repository.update_job(&job).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::IdGenerator;
    use domain::shared::ids::{TenantId, UserId};
    use domain::value_objects::ProxyHost;
    use infra::{InMemoryProxyCheckJobRepository, UuidIdGenerator};
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn run_proxy_check_job_marks_job_completed_and_records_entry_result() {
        let repo = InMemoryProxyCheckJobRepository::new();
        let id_generator = UuidIdGenerator::new();

        let user_id = UserId::new();
        let job_id = id_generator.next_proxy_check_job_id().await;
        let job = ProxyCheckJob::new(job_id, user_id, TenantId::new(), 1);

        let entry_id = id_generator.next_proxy_entry_id().await;
        let host = ProxyHost::new("localhost").unwrap();
        let entry = ProxyEntry::new(entry_id, job_id, host, 8080, ProxyScheme::Http, None).unwrap();

        repo.save_job(&job, &[entry]).await.unwrap();

        // The worker owns the atomic Pending -> Running claim.
        assert!(repo.try_mark_running(job_id, "test-worker").await.unwrap());

        let running_job = repo
            .find_job_by_id(job_id, job.tenant_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(running_job.status(), domain::ProxyCheckJobStatus::Running);

        let entries = repo
            .find_entries_by_job_id(job_id, job.tenant_id())
            .await
            .unwrap();

        run_proxy_check_job(&repo, running_job, entries, "test-worker", None)
            .await
            .unwrap();

        let job_after = repo
            .find_job_by_id(job_id, job.tenant_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job_after.status(), domain::ProxyCheckJobStatus::Completed);
        assert_eq!(job_after.checked_count(), 1);

        let entries_after = repo
            .find_entries_by_job_id(job_id, job.tenant_id())
            .await
            .unwrap();
        assert_eq!(entries_after.len(), 1);
        assert!(entries_after[0].result().is_some());
    }

    #[test]
    fn parse_status_code_accepts_200() {
        assert_eq!(parse_status_code("HTTP/1.1 200 OK\r\n\r\n").unwrap(), 200);
    }

    #[test]
    fn parse_status_code_rejects_non_http() {
        assert!(parse_status_code("SOCKS5\r\n").is_err());
    }

    #[tokio::test]
    async fn http_proxy_verification_accepts_2xx() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await;
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let creds = ProxyCredentials::new("user", "pass").ok();

        assert!(
            verify_http_proxy_absolute_form(&mut stream, creds.as_ref(), Duration::from_secs(2))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn http_proxy_verification_rejects_oversized_headers() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream.read(&mut [0u8; 4096]).await;
                let payload = vec![b'a'; MAX_PROXY_RESPONSE_HEADER_BYTES + 1024];
                let _ = stream.write_all(&payload).await;
                let _ = stream.flush().await;
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_proxy_absolute_form(&mut stream, None, Duration::from_secs(2)).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("size limit"));
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/proxy_check_worker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/proxy_check_worker.rs")"
  cp "crates/worker/src/proxy_check_worker.rs" "$BACKUP_DIR/crates/worker/src/proxy_check_worker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/proxy_check_worker.rs")"
cat > "crates/worker/src/proxy_check_worker.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::proxy_checker::run_proxy_check_job;
use application::ports::ProxyCheckJobRepository;
use application::MeterUsageUseCase;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

const PROXY_CHECK_POLL_INTERVAL_SECS: u64 = 5;
const MAX_JOBS_PER_SWEEP: usize = 10;
const PROXY_JOB_LEASE_SECS: i64 = 60;
const PROXY_JOB_MAX_RETRIES: u32 = 3;

#[derive(Clone)]
pub struct ProxyCheckWorker {
    repository: Arc<dyn ProxyCheckJobRepository>,
    metering: Arc<MeterUsageUseCase>,
    worker_id: String,
}
impl ProxyCheckWorker {
    pub fn new(
        repository: Arc<dyn ProxyCheckJobRepository>,
        metering: Arc<MeterUsageUseCase>,
    ) -> Self {
        Self {
            repository,
            metering,
            worker_id: Uuid::new_v4().to_string(),
        }
    }

    pub async fn run_once(&self) -> Result<()> {
        let stale_before = chrono::Utc::now() - chrono::Duration::seconds(PROXY_JOB_LEASE_SECS);
        if let Err(err) = self
            .repository
            .recover_stale_jobs(stale_before, PROXY_JOB_MAX_RETRIES)
            .await
        {
            error!(error=%err, "proxy stale-job recovery failed");
        }

        let jobs = self
            .repository
            .find_pending_jobs(MAX_JOBS_PER_SWEEP)
            .await?;
        for candidate in jobs {
            let job_id = candidate.id();

            // Atomic claim is the single source of truth for ownership.
            // No executor-side mark_running is allowed.
            if !self
                .repository
                .try_mark_running(job_id, &self.worker_id)
                .await?
            {
                continue;
            }

            let tenant_id = candidate.tenant_id();
            let running_job = match self.repository.find_job_by_id(job_id, tenant_id).await {
                Ok(Some(job)) if job.status() == domain::ProxyCheckJobStatus::Running => job,
                Ok(Some(_)) => continue,
                Ok(None) => {
                    error!(job_id=%job_id, "claimed proxy check job disappeared");
                    continue;
                }
                Err(err) => {
                    error!(job_id=%job_id, error=%err, "failed to reload claimed proxy check job");
                    let mut failed = candidate.clone();
                    failed.set_worker_lease(self.worker_id.clone(), chrono::Utc::now());
                    if failed.mark_failed("failed to reload claimed proxy check job") {
                        let _ = self.repository.update_job(&failed).await;
                    }
                    continue;
                }
            };

            let entries = match self
                .repository
                .find_entries_by_job_id(job_id, tenant_id)
                .await
            {
                Ok(entries) => entries,
                Err(err) => {
                    let mut failed = running_job;
                    if failed.mark_failed(format!("failed to load proxy entries: {err}"))
                        && let Err(update_err) = self.repository.update_job(&failed).await
                    {
                        error!(
                            job_id=%job_id,
                            error=%update_err,
                            "failed to persist entry-load failure"
                        );
                    }
                    continue;
                }
            };

            // A zero-entry job and a malformed persisted job do not perform a
            // network attempt, so they must never consume PAYG credit.
            if entries.is_empty() || entries.len() != running_job.total_count() as usize {
                if let Err(err) =
                    run_proxy_check_job(&*self.repository, running_job, entries, &self.worker_id, None)
                        .await
                {
                    error!(job_id=%job_id, error=%err, "proxy preflight failure");
                }
                continue;
            }

            // Per-entry billing now lives inside run_proxy_check_job, where the
            // durable job + entry identity is available. This prevents a bulk
            // proxy job from being charged once for many entries and keeps
            // retries idempotent per logical proxy attempt.
            let result = run_proxy_check_job(
                &*self.repository,
                running_job,
                entries,
                &self.worker_id,
                Some(self.metering.clone()),
            ).await;

            if let Err(err) = result {
                let reason = format!("proxy billing/execution failed: {err}");
                error!(job_id=%job_id, error=%err, "proxy check executor failed");
                // The executor normally terminalizes the job itself. This
                // fallback prevents an unexpected propagated error from
                // leaving a claimed job in running forever and preserves a
                // safe, user-visible billing reason.
                if let Ok(Some(mut latest)) =
                    self.repository.find_job_by_id(job_id, tenant_id).await
                    && latest.status() == domain::ProxyCheckJobStatus::Running
                    && latest.mark_failed(reason)
                    && let Err(update_err) = self.repository.update_job(&latest).await
                {
                    error!(
                        job_id=%job_id,
                        error=%update_err,
                        "failed to persist proxy worker failure"
                    );
                }
            }
        }
        Ok(())
    }

    pub async fn run_forever(&self, token: CancellationToken) -> Result<()> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(PROXY_CHECK_POLL_INTERVAL_SECS));
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("proxy check worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = interval.tick() => {
                    if let Err(err) = self.run_once().await {
                        error!(error=%err, "proxy check worker sweep failed");
                    }
                }
            }
        }
    }
}
PINGVERTEX_EOF

if [[ -f "crates/domain/src/entities/proxy_check_job.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/domain/src/entities/proxy_check_job.rs")"
  cp "crates/domain/src/entities/proxy_check_job.rs" "$BACKUP_DIR/crates/domain/src/entities/proxy_check_job.rs"
fi

mkdir -p "$(dirname "crates/domain/src/entities/proxy_check_job.rs")"
cat > "crates/domain/src/entities/proxy_check_job.rs" <<'PINGVERTEX_EOF'
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    enums::ProxyCheckJobStatus,
    shared::{
        ids::{ProxyCheckJobId, TenantId, UserId},
        policies::{Ownable, TenantOwned},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyCheckJob {
    id: ProxyCheckJobId,
    user_id: UserId,
    tenant_id: TenantId,
    status: ProxyCheckJobStatus,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    heartbeat_at: Option<DateTime<Utc>>,
    worker_id: Option<String>,
    completed_at: Option<DateTime<Utc>>,
    canceled_at: Option<DateTime<Utc>>,
    failed_at: Option<DateTime<Utc>>,
    failure_reason: Option<String>,
    retry_count: u32,
    total_count: u32,
    checked_count: u32,
    successful_count: u32,
    failed_count: u32,
}

impl ProxyCheckJob {
    pub fn new(
        id: ProxyCheckJobId,
        user_id: UserId,
        tenant_id: TenantId,
        total_count: u32,
    ) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            status: ProxyCheckJobStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            heartbeat_at: None,
            worker_id: None,
            completed_at: None,
            canceled_at: None,
            failed_at: None,
            failure_reason: None,
            retry_count: 0,
            total_count,
            checked_count: 0,
            successful_count: 0,
            failed_count: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_persisted(
        id: ProxyCheckJobId,
        user_id: UserId,
        tenant_id: TenantId,
        status: ProxyCheckJobStatus,
        created_at: DateTime<Utc>,
        started_at: Option<DateTime<Utc>>,
        heartbeat_at: Option<DateTime<Utc>>,
        worker_id: Option<String>,
        completed_at: Option<DateTime<Utc>>,
        canceled_at: Option<DateTime<Utc>>,
        failed_at: Option<DateTime<Utc>>,
        failure_reason: Option<String>,
        retry_count: u32,
        total_count: u32,
        checked_count: u32,
        successful_count: u32,
        failed_count: u32,
    ) -> Self {
        Self {
            id,
            user_id,
            tenant_id,
            status,
            created_at,
            started_at,
            heartbeat_at,
            worker_id,
            completed_at,
            canceled_at,
            failed_at,
            failure_reason,
            retry_count,
            total_count,
            checked_count,
            successful_count,
            failed_count,
        }
    }

    pub fn id(&self) -> ProxyCheckJobId {
        self.id
    }
    pub fn user_id(&self) -> UserId {
        self.user_id
    }
    pub fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
    pub fn status(&self) -> ProxyCheckJobStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }
    pub fn heartbeat_at(&self) -> Option<DateTime<Utc>> {
        self.heartbeat_at
    }
    pub fn worker_id(&self) -> Option<&str> {
        self.worker_id.as_deref()
    }
    pub fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
    pub fn canceled_at(&self) -> Option<DateTime<Utc>> {
        self.canceled_at
    }
    pub fn failed_at(&self) -> Option<DateTime<Utc>> {
        self.failed_at
    }
    pub fn failure_reason(&self) -> Option<&str> {
        self.failure_reason.as_deref()
    }
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }
    pub fn total_count(&self) -> u32 {
        self.total_count
    }
    pub fn checked_count(&self) -> u32 {
        self.checked_count
    }
    pub fn successful_count(&self) -> u32 {
        self.successful_count
    }
    pub fn failed_count(&self) -> u32 {
        self.failed_count
    }

    /// Used by domain-only tests and non-persistent callers. Production
    /// workers claim atomically through the repository before executing.
    pub fn mark_running(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Pending {
            return false;
        }
        self.status = ProxyCheckJobStatus::Running;
        self.started_at = Some(Utc::now());
        self.heartbeat_at = self.started_at;
        self.completed_at = None;
        self.canceled_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        true
    }

    pub fn set_worker_lease(&mut self, worker_id: impl Into<String>, heartbeat_at: DateTime<Utc>) {
        self.status = ProxyCheckJobStatus::Running;
        if self.started_at.is_none() {
            self.started_at = Some(heartbeat_at);
        }
        self.heartbeat_at = Some(heartbeat_at);
        self.worker_id = Some(worker_id.into());
        self.completed_at = None;
        self.canceled_at = None;
        self.failed_at = None;
        self.failure_reason = None;
    }

    /// Rebuilds progress counters from the durable entry results before a retry.
    /// This prevents a crash between entry persistence and job-progress persistence
    /// from either double-counting or falsely failing an otherwise complete job.
    pub fn rebuild_progress(&mut self, successful: u32, failed: u32) {
        let checked = successful.saturating_add(failed).min(self.total_count);
        self.successful_count = successful.min(checked);
        self.failed_count = checked.saturating_sub(self.successful_count);
        self.checked_count = checked;
    }

    pub fn record_result(&mut self, successful: bool) {
        if self.checked_count >= self.total_count {
            return;
        }
        self.checked_count += 1;
        if successful {
            self.successful_count = self.successful_count.saturating_add(1);
        } else {
            self.failed_count = self.failed_count.saturating_add(1);
        }
    }

    pub fn mark_completed(&mut self) -> bool {
        if self.status != ProxyCheckJobStatus::Running {
            return false;
        }
        self.status = ProxyCheckJobStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.canceled_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        self.heartbeat_at = None;
        self.worker_id = None;
        true
    }

    pub fn mark_failed(&mut self, reason: impl Into<String>) -> bool {
        if self.status != ProxyCheckJobStatus::Running {
            return false;
        }
        let mut reason = reason.into();
        if reason.is_empty() {
            reason = "proxy check job failed".to_owned();
        }
        reason.truncate(1024);
        self.status = ProxyCheckJobStatus::Failed;
        self.completed_at = None;
        self.canceled_at = None;
        self.failed_at = Some(Utc::now());
        self.failure_reason = Some(reason);
        self.heartbeat_at = None;
        self.worker_id = None;
        true
    }

    pub fn mark_canceled(&mut self) -> bool {
        if !matches!(
            self.status,
            ProxyCheckJobStatus::Pending | ProxyCheckJobStatus::Running
        ) {
            return false;
        }
        self.status = ProxyCheckJobStatus::Canceled;
        self.completed_at = None;
        self.failed_at = None;
        self.failure_reason = None;
        self.canceled_at = Some(Utc::now());
        self.heartbeat_at = None;
        self.worker_id = None;
        true
    }
}

impl Ownable for ProxyCheckJob {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}
impl TenantOwned for ProxyCheckJob {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}
PINGVERTEX_EOF

if [[ -f "crates/application/src/use_cases/resolve_entitlements.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/src/use_cases/resolve_entitlements.rs")"
  cp "crates/application/src/use_cases/resolve_entitlements.rs" "$BACKUP_DIR/crates/application/src/use_cases/resolve_entitlements.rs"
fi

mkdir -p "$(dirname "crates/application/src/use_cases/resolve_entitlements.rs")"
cat > "crates/application/src/use_cases/resolve_entitlements.rs" <<'PINGVERTEX_EOF'
use std::error::Error;

use crate::{ports::{PricingConfigRepository, SubscriptionRepository}, queries::ResolveEntitlements};
use domain::{Plan, enums::PlanTier, enums::SubscriptionStatus};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Resolves the current entitlements (plan limits) for a tenant.
///
/// A reusable service, not tied to any HTTP handler, so later workers
/// (quota enforcement at resource creation, billing) can all call the
/// same resolution logic instead of re-deriving it.
///
/// A tenant with no `Subscription` row is treated as being on the Free
/// plan. As of B12, a subscription's *status* also affects resolution:
/// `Trialing`, `Active`, and `PastDue` (grace period) all resolve full
/// entitlements for the subscribed tier — grace period is a soft
/// landing, not an immediate cutoff. `Expired` and `Canceled`
/// subscriptions fall back to Free, same as having no row at all.
pub struct ResolveEntitlementsUseCase<R> {
    repository: R,
    pricing_repository: Option<std::sync::Arc<dyn PricingConfigRepository>>,
}

impl<R> ResolveEntitlementsUseCase<R>
where
    R: SubscriptionRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository, pricing_repository: None }
    }

    pub fn with_pricing_config_repository(
        mut self, repository: std::sync::Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_repository = Some(repository);
        self
    }

    pub async fn execute(&self, query: ResolveEntitlements) -> UseCaseResult<Plan> {
        if let Some(test_tier) = self.repository.find_test_plan_override(query.tenant_id).await? {
            let mut plan = Plan::for_tier(test_tier);
            if test_tier == PlanTier::Free {
                if let Some(pricing_repository) = &self.pricing_repository {
                    if let Some(config) = pricing_repository.find_active_config().await? {
                        plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                        plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                        plan.history_retention_days = config.free_tier.retention_days;
                    }
                }
            }
            return Ok(plan);
        }

        let plan_tier = self
            .repository
            .find_by_tenant_id(query.tenant_id)
            .await?
            .map(|subscription| match subscription.status {
                SubscriptionStatus::Expired | SubscriptionStatus::Canceled => PlanTier::Free,
                SubscriptionStatus::Trialing
                | SubscriptionStatus::Active
                | SubscriptionStatus::PastDue => subscription.plan_tier,
            })
            .unwrap_or(PlanTier::Free);

        let mut plan = Plan::for_tier(plan_tier);
        if plan_tier == PlanTier::Free {
            if let Some(pricing_repository) = &self.pricing_repository {
                if let Some(config) = pricing_repository.find_active_config().await? {
                    plan.max_monitors = config.free_tier.monitors.min(u32::MAX as u64) as u32;
                    plan.min_check_interval_seconds = config.free_tier.interval_seconds;
                    plan.history_retention_days = config.free_tier.retention_days;
                }
            }
        }
        Ok(plan)
    }
}
PINGVERTEX_EOF

if [[ -f "crates/application/src/use_cases/create_monitor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/src/use_cases/create_monitor.rs")"
  cp "crates/application/src/use_cases/create_monitor.rs" "$BACKUP_DIR/crates/application/src/use_cases/create_monitor.rs"
fi

mkdir -p "$(dirname "crates/application/src/use_cases/create_monitor.rs")"
cat > "crates/application/src/use_cases/create_monitor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateMonitorRequest,
    ports::{
        AuditLogEntryRepository, IdGenerator, MonitorRepository, QuotaCheckOutcome,
        SubscriptionRepository,
    },
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, HeartbeatToken,
    Host, Monitor, MonitorBuilder, MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Backwards-compatible fallback cap used only when this use case is constructed
/// without `.with_entitlements(...)`. The production API always enables the
/// atomic, tenant-scoped entitlement path. Keep this value aligned with the
/// product Free-tier monitor limit so direct/unit-test construction cannot
/// accidentally reintroduce the old 10-monitor contract.
pub const MAX_MONITORS_PER_USER: usize = 20;

/// Backwards-compatible fallback minimum interval for direct/unit-test
/// construction without an entitlement repository. This mirrors the current
/// Free-tier contract; production requests resolve the value from the plan.
const DEFAULT_FREE_MIN_INTERVAL_SECONDS: u32 = 300;
/// Hard cap on the number of assertions a single monitor may be created with.
pub const MAX_ASSERTIONS_PER_MONITOR: usize = 20;

#[derive(Debug)]
pub enum CreateMonitorError {
    Forbidden,
    QuotaExceeded { limit: usize },
    TooManyAssertions { limit: usize },
    IntervalBelowPlanMinimum { min: u32 },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create monitors"
            ),
            Self::QuotaExceeded { limit } => {
                write!(
                    f,
                    "monitor limit reached: your plan allows at most {limit} monitors"
                )
            }
            Self::TooManyAssertions { limit } => {
                write!(
                    f,
                    "too many assertions: a monitor allows at most {limit} assertions"
                )
            }
            Self::IntervalBelowPlanMinimum { min } => {
                write!(
                    f,
                    "interval must be at least {min} seconds on your current plan"
                )
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Forbidden => None,
            Self::QuotaExceeded { .. } => None,
            Self::TooManyAssertions { .. } => None,
            Self::IntervalBelowPlanMinimum { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateMonitorUseCase<R, G> {
    repository: R,
    id_generator: G,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    subscription_repository: Option<Arc<dyn SubscriptionRepository>>,
    pricing_config_repository: Option<Arc<dyn PricingConfigRepository>>,
}

impl<R, G> CreateMonitorUseCase<R, G>
where
    R: MonitorRepository,
    G: IdGenerator,
{
    pub fn new(
        repository: R,
        id_generator: G,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            audit_log_entry_repository,
            subscription_repository: None,
            pricing_config_repository: None,
        }
    }

    /// Enables B13's atomic, tenant-scoped, plan-entitlement-based quota
    /// enforcement and plan-minimum interval validation.
    ///
    /// When this is NOT called, `execute` retains the legacy non-atomic
    /// fallback used by older direct/unit-test callers. Its limits are kept
    /// aligned with the current Free-tier contract.
    pub fn with_entitlements(
        mut self,
        subscription_repository: Arc<dyn SubscriptionRepository>,
    ) -> Self {
        self.subscription_repository = Some(subscription_repository);
        self
    }

    pub fn with_pricing_config_repository(
        mut self, repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_config_repository = Some(repository);
        self
    }

    pub async fn execute(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Monitor, CreateMonitorError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateMonitorError::Forbidden);
        }

        if request.assertions.len() > MAX_ASSERTIONS_PER_MONITOR {
            return Err(CreateMonitorError::TooManyAssertions {
                limit: MAX_ASSERTIONS_PER_MONITOR,
            });
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let (monitor_limit, min_interval) = match &self.subscription_repository {
            Some(repo) => {
                let plan = {
                    let mut resolver = ResolveEntitlementsUseCase::new(repo.clone());
                    if let Some(pricing) = &self.pricing_config_repository {
                        resolver = resolver.with_pricing_config_repository(pricing.clone());
                    }
                    resolver.execute(ResolveEntitlements { tenant_id })
                    .await
                    .map_err(CreateMonitorError::Other)?
                }
                (plan.max_monitors, plan.min_check_interval_seconds)
            }
            None => (
                MAX_MONITORS_PER_USER as u32,
                DEFAULT_FREE_MIN_INTERVAL_SECONDS,
            ),
        };

        if request.interval_seconds < min_interval {
            return Err(CreateMonitorError::IntervalBelowPlanMinimum { min: min_interval });
        }

        let monitor = self
            .build_monitor(request, &authenticated_user)
            .await
            .map_err(CreateMonitorError::Other)?;

        match &self.subscription_repository {
            Some(_subscription_repository) => {
                match self
                    .repository
                    .save_if_within_quota(&monitor, tenant_id, monitor_limit)
                    .await
                    .map_err(CreateMonitorError::Other)?
                {
                    QuotaCheckOutcome::Saved => {}
                    QuotaCheckOutcome::QuotaExceeded => {
                        return Err(CreateMonitorError::QuotaExceeded {
                            limit: monitor_limit as usize,
                        });
                    }
                }
            }
            None => {
                let existing_for_user = self
                    .repository
                    .find_all_for_user(user_id)
                    .await
                    .map_err(CreateMonitorError::Other)?;

                if existing_for_user.len() >= monitor_limit as usize {
                    return Err(CreateMonitorError::QuotaExceeded {
                        limit: monitor_limit as usize,
                    });
                }

                self.repository
                    .save(&monitor)
                    .await
                    .map_err(CreateMonitorError::Other)?;
            }
        }

        self.record_audit_log(&monitor, user_id, tenant_id)
            .await
            .map_err(CreateMonitorError::Other)?;

        Ok(monitor)
    }

    async fn build_monitor(
        &self,
        request: CreateMonitorRequest,
        authenticated_user: &AuthenticatedUser,
    ) -> UseCaseResult<Monitor> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let id = self.id_generator.next_monitor_id().await;

        let target = match request.kind {
            MonitorKind::Http | MonitorKind::Heartbeat => {
                Url::new(request.url)?.as_str().to_owned()
            }
            MonitorKind::Tcp | MonitorKind::Dns => Host::new(request.url)?.as_str().to_owned(),
            MonitorKind::Flow => request.url,
        };

        let mut builder = MonitorBuilder::new(id, user_id, tenant_id)
            .name(MonitorName::new(request.name)?)
            .target(target)
            .method(request.method)
            .expected_status(ExpectedStatus::new(request.expected_status)?)
            .interval(CheckInterval::new(request.interval_seconds)?)
            .timeout(Timeout::new(request.timeout_seconds)?)
            .kind(request.kind);

        if let Some(tcp_port) = request.tcp_port {
            builder = builder.tcp_port(tcp_port);
        }
        if let Some(dns_record_type) = request.dns_record_type {
            builder = builder.dns_record_type(dns_record_type);
        }
        if let Some(dns_expected_value) = request.dns_expected_value {
            builder = builder.dns_expected_value(dns_expected_value);
        }
        if request.kind == MonitorKind::Heartbeat {
            builder = builder.heartbeat_token(HeartbeatToken::generate());
            if let Some(interval) = request.heartbeat_interval_seconds {
                builder = builder.heartbeat_interval_seconds(interval);
            }
            if let Some(grace) = request.heartbeat_grace_period_seconds {
                builder = builder.heartbeat_grace_period_seconds(grace);
            }
        }

        let mut monitor = builder.try_build()?;

        monitor.is_public = request.is_public;
        monitor.ssl_check_enabled = request.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = request.ssl_expiry_warning_days;

        for input in &request.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;
            monitor
                .add_assertion(assertion)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
        }

        Ok(monitor)
    }

    async fn record_audit_log(
        &self,
        monitor: &Monitor,
        user_id: UserId,
        tenant_id: TenantId,
    ) -> UseCaseResult<()> {
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorCreated,
            target_id: monitor.id.0.to_string(),
            metadata: json!({
                "name": monitor.name.as_str(),
                "is_public": monitor.is_public,
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;
        Ok(())
    }
}
PINGVERTEX_EOF

if [[ -f "crates/application/src/use_cases/update_monitor.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/src/use_cases/update_monitor.rs")"
  cp "crates/application/src/use_cases/update_monitor.rs" "$BACKUP_DIR/crates/application/src/use_cases/update_monitor.rs"
fi

mkdir -p "$(dirname "crates/application/src/use_cases/update_monitor.rs")"
cat > "crates/application/src/use_cases/update_monitor.rs" <<'PINGVERTEX_EOF'
use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateMonitor,
    ports::{AuditLogEntryRepository, MonitorRepository, SubscriptionRepository, PricingConfigRepository},
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, CheckInterval, ExpectedStatus, Host, Monitor,
    MonitorKind, MonitorName, Timeout, Url,
    entities::Assertion,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateMonitorUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    subscription_repository: Option<Arc<dyn SubscriptionRepository>>,
    pricing_config_repository: Option<Arc<dyn PricingConfigRepository>>,
}

impl<R> UpdateMonitorUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(
        repository: R,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            audit_log_entry_repository,
            subscription_repository: None,
            pricing_config_repository: None,
        }
    }

    /// Enables plan-minimum interval validation during updates.
    /// When not called, falls back to global `CheckInterval::MIN`.
    pub fn with_entitlements(
        mut self,
        subscription_repository: Arc<dyn SubscriptionRepository>,
    ) -> Self {
        self.subscription_repository = Some(subscription_repository);
        self
    }

    pub fn with_pricing_config_repository(
        mut self, repository: Arc<dyn PricingConfigRepository>,
    ) -> Self {
        self.pricing_config_repository = Some(repository);
        self
    }

    pub async fn execute(
        &self,
        command: UpdateMonitor,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<Monitor>> {
        authenticated_user.require(Permission::UpdateMonitor)?;

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut monitor = match self.repository.find_by_id(command.id).await? {
            Some(monitor) => monitor,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Ok(None);
        }

        // Resolve min interval (plan or fallback)
        let min_interval = match &self.subscription_repository {
            Some(repo) => {
                let plan = {
                    let mut resolver = ResolveEntitlementsUseCase::new(repo.clone());
                    if let Some(pricing) = &self.pricing_config_repository {
                        resolver = resolver.with_pricing_config_repository(pricing.clone());
                    }
                    resolver.execute(ResolveEntitlements { tenant_id })
                    .await?
                }
                plan.min_check_interval_seconds
            }
            None => CheckInterval::MIN,
        };

        if command.interval_seconds < min_interval {
            return Err(format!(
                "interval must be at least {min_interval} seconds on your current plan"
            )
            .into());
        }

        // Validate target based on kind, same as create
        let target = match command.kind {
            MonitorKind::Http | MonitorKind::Heartbeat => {
                Url::new(command.url)?.as_str().to_owned()
            }
            MonitorKind::Tcp | MonitorKind::Dns => Host::new(command.url)?.as_str().to_owned(),
            MonitorKind::Flow => command.url,
        };

        monitor.update(
            MonitorName::new(command.name)?,
            target,
            command.method,
            ExpectedStatus::new(command.expected_status)?,
            CheckInterval::new(command.interval_seconds)?,
            Timeout::new(command.timeout_seconds)?,
        );

        monitor
            .update_kind_fields(
                command.kind,
                command.tcp_port,
                command.dns_record_type,
                command.dns_expected_value,
                command.heartbeat_interval_seconds,
                command.heartbeat_grace_period_seconds,
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        monitor.is_public = command.is_public;
        monitor.ssl_check_enabled = command.ssl_check_enabled;
        monitor.ssl_expiry_warning_days = command.ssl_expiry_warning_days;

        monitor.assertions.clear();

        for input in &command.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

            monitor
                .add_assertion(assertion)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
        }

        self.repository.save(&monitor).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MonitorUpdated,
            target_id: monitor.id.0.to_string(),
            metadata: json!({
                "name": monitor.name.as_str(),
                "is_public": monitor.is_public,
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(monitor))
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/retention_worker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/retention_worker.rs")"
  cp "crates/worker/src/retention_worker.rs" "$BACKUP_DIR/crates/worker/src/retention_worker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/retention_worker.rs")"
cat > "crates/worker/src/retention_worker.rs" <<'PINGVERTEX_EOF'
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use application::{
    ports::{HistoryRetentionRepository, PricingConfigRepository, SubscriptionRepository, TenantRepository},
    queries::ResolveEntitlements,
    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
};
use chrono::{Duration as ChronoDuration, Utc};
use tokio::{sync::Semaphore, task::JoinSet, time};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub const DEFAULT_RETENTION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const DEFAULT_BATCH_SIZE: u32 = 1000;

/// Result summary for one sweep.
#[derive(Debug, Clone, Default)]
pub struct SweepSummary {
    pub tenants_processed: u64,
    pub checks_deleted: u64,
}

pub struct RetentionWorker<T, S, H, P>
where
    T: TenantRepository,
    S: SubscriptionRepository + 'static,
    H: HistoryRetentionRepository + 'static,
    P: PricingConfigRepository + 'static,
{
    tenant_repository: Arc<T>,
    subscription_repository: Arc<S>,
    history_repository: Arc<H>,
    pricing_repository: Arc<P>,
    batch_size: u32,
    interval: Duration,
    max_concurrent_tenants: usize,
}

impl<T, S, H, P> RetentionWorker<T, S, H, P>
where
    T: TenantRepository,
    S: SubscriptionRepository + 'static,
    H: HistoryRetentionRepository + 'static,
    P: PricingConfigRepository + 'static,
{
    pub fn new(
        tenant_repository: Arc<T>,
        subscription_repository: Arc<S>,
        history_repository: Arc<H>,
        pricing_repository: Arc<P>,
        batch_size: u32,
        interval: Duration,
        max_concurrent_tenants: usize,
    ) -> Self {
        Self {
            tenant_repository,
            subscription_repository,
            history_repository,
            pricing_repository,
            batch_size,
            interval,
            max_concurrent_tenants,
        }
    }

    /// Runs one full retention sweep over all tenants with bounded
    /// concurrency. Each tenant is processed independently; failures for
    /// an individual tenant are logged and skipped.
    pub async fn run_sweep(&self) -> Result<SweepSummary, Box<dyn Error + Send + Sync>> {
        let tenants = self.tenant_repository.list_all().await?;
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_tenants.max(1)));
        let mut join_set = JoinSet::new();
        let mut summary = SweepSummary::default();
        let checks_deleted = Arc::new(AtomicU64::new(0));

        for tenant in tenants {
            let permit = semaphore.clone().acquire_owned().await?;
            let subscription_repository = self.subscription_repository.clone();
            let history_repository = self.history_repository.clone();
            let pricing_repository = self.pricing_repository.clone();
            let batch_size = self.batch_size;
            let checks_deleted = checks_deleted.clone();

            join_set.spawn(async move {
                let _permit = permit;

                let plan = match ResolveEntitlementsUseCase::new(subscription_repository)
                    .with_pricing_config_repository(pricing_repository)
                    .execute(ResolveEntitlements {
                        tenant_id: tenant.id,
                    })
                    .await
                {
                    Ok(plan) => plan,
                    Err(err) => {
                        error!(
                            tenant_id = ?tenant.id,
                            error = %err,
                            "failed to resolve entitlements for tenant; skipping retention"
                        );
                        return;
                    }
                };

                let retention_days = plan.history_retention_days;
                let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);

                match history_repository
                    .delete_older_than_batched(tenant.id, cutoff, batch_size)
                    .await
                {
                    Ok(deleted) => {
                        if deleted > 0 {
                            checks_deleted.fetch_add(deleted, Ordering::Relaxed);
                            info!(
                                tenant_id = ?tenant.id,
                                retention_days,
                                deleted,
                                "retention sweep deleted old checks"
                            );
                        }
                    }
                    Err(err) => {
                        error!(
                            tenant_id = ?tenant.id,
                            error = %err,
                            "retention delete failed for tenant"
                        );
                    }
                }
            });
        }

        while join_set.join_next().await.is_some() {
            summary.tenants_processed += 1;
        }

        summary.checks_deleted = checks_deleted.load(Ordering::Relaxed);

        Ok(summary)
    }

    pub async fn run_forever(
        &self,
        token: CancellationToken,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!(
            interval_secs = self.interval.as_secs(),
            batch_size = self.batch_size,
            max_concurrent_tenants = self.max_concurrent_tenants,
            "retention worker started"
        );

        let mut ticker = time::interval(self.interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("retention worker cancellation requested; shutting down");
                    return Ok(());
                }
                _ = ticker.tick() => {
                    match self.run_sweep().await {
                        Ok(summary) => {
                            info!(
                                tenants_processed = summary.tenants_processed,
                                checks_deleted = summary.checks_deleted,
                                "retention sweep completed"
                            );
                        }
                        Err(err) => {
                            error!(error = %err, "retention sweep failed");
                        }
                    }
                }
            }
        }
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/reconciliation_worker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/reconciliation_worker.rs")"
  cp "crates/worker/src/reconciliation_worker.rs" "$BACKUP_DIR/crates/worker/src/reconciliation_worker.rs"
fi

mkdir -p "$(dirname "crates/worker/src/reconciliation_worker.rs")"
cat > "crates/worker/src/reconciliation_worker.rs" <<'PINGVERTEX_EOF'
use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::time::Duration as StdDuration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
pub struct ReconciliationWorker {
    pool: PgPool,
    interval: StdDuration,
}
impl ReconciliationWorker {
    pub fn new(pool: PgPool, interval: StdDuration) -> Self {
        Self { pool, interval }
    }
    pub async fn run_forever(
        self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.run_once().await?;
        let mut tick = tokio::time::interval(self.interval);
        loop {
            tokio::select! {_=cancel.cancelled()=>break,_=tick.tick()=>{if let Err(e)=self.run_once().await{error!(error=%e,"billing reconciliation failed");}}}
        }
        Ok(())
    }
    pub async fn run_once(
        &self,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let started = Utc::now();
        let end = started;
        let start = end - Duration::hours(25);
        // Provider-side truth available to this codebase is the durable, signed webhook observation log.
        // A future live-provider adapter can replace/augment this query without changing report shape.
        let missing_payment: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_audit_log b WHERE b.created_at >= $1 AND b.created_at < $2 AND b.event_type IN ('payment_succeeded','subscription_renewed') AND ((b.payload->>'payment_id') IS NULL OR NOT EXISTS (SELECT 1 FROM payments p WHERE p.tenant_id=b.tenant_id AND p.provider_payment_id=b.payload->>'payment_id'))"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let duplicate_payment: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT provider, provider_payment_id FROM payments WHERE provider_payment_id IS NOT NULL AND created_at >= $1 AND created_at < $2 GROUP BY provider, provider_payment_id HAVING COUNT(*) > 1) d"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let incorrect_amount: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_audit_log b JOIN payments p ON p.tenant_id=b.tenant_id AND p.provider_payment_id=b.payload->>'payment_id' WHERE b.created_at >= $1 AND b.created_at < $2 AND b.event_type IN ('payment_succeeded','subscription_renewed') AND ((NOT (b.payload ? 'amount_units')) OR NOT ((b.payload->>'amount_units') ~ '^[0-9]+$') OR (b.payload->>'amount_units')::numeric <> p.amount_units)"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let orphan_webhook: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM billing_audit_log b WHERE b.created_at >= $1 AND b.created_at < $2 AND (
                (b.event_type IN ('payment_succeeded','payment_failed','payment_refunded','subscription_renewed')
                 AND (b.payload->>'payment_id') IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM payments p WHERE p.tenant_id=b.tenant_id AND p.provider_payment_id=b.payload->>'payment_id'))
                OR
                (b.event_type='subscription_canceled'
                 AND NOT EXISTS (SELECT 1 FROM subscriptions s WHERE s.tenant_id=b.tenant_id))
                OR
                ((b.payload->>'wallet_top_up') = 'true')
                 AND (b.payload->>'provider_checkout_id') IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM wallet_top_up_sessions w WHERE w.tenant_id=b.tenant_id AND w.provider_checkout_id=b.payload->>'provider_checkout_id')
            )"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let wallet_mismatch: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_wallets w WHERE w.updated_at >= $1 AND w.updated_at < $2 AND (w.balance_units <> COALESCE((SELECT SUM(wt.amount_units) FROM wallet_transactions wt WHERE wt.wallet_id=w.id),0) OR EXISTS (SELECT 1 FROM wallet_top_up_sessions s WHERE s.tenant_id=w.tenant_id AND s.created_at >= $1 AND s.created_at < $2 AND s.status='COMPLETED' AND s.provider_payment_id IS NULL))"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        let invoice_mismatch: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invoices i WHERE i.created_at >= $1 AND i.created_at < $2 AND ((i.status='paid' AND i.paid_at IS NULL) OR (i.status <> 'paid' AND i.paid_at IS NOT NULL) OR i.total < 0 OR i.subtotal < 0 OR EXISTS (SELECT 1 FROM invoice_lines l WHERE l.invoice_id=i.id AND l.amount <> l.quantity*l.unit_price))"
        )
        .bind(start).bind(end).fetch_one(&self.pool).await?;
        // PAYG financial reconciliation. A crash in an older deployment could
        // leave a committed debit without its immutable usage/audit records.
        // New captures are atomic; this pass repairs legacy gaps using the
        // historical price effective at the debit timestamp.
        let payg_usage_repaired: i64 = sqlx::query_scalar::<sqlx::Postgres, i64>(
            r#"
            WITH candidates AS (
                SELECT r.id reservation_id, r.request_id, r.operation,
                       w.tenant_id, w.currency, -wt.amount_units amount_units,
                       wt.created_at
                FROM credit_reservations r
                JOIN user_wallets w ON w.id=r.wallet_id
                JOIN wallet_transactions wt
                  ON wt.reference_type='credit_reservation'
                 AND wt.reference_id=r.id::text
                 AND wt.transaction_type='DEBIT'
                LEFT JOIN api_usage_events e
                  ON e.tenant_id=w.tenant_id AND e.request_id=r.request_id
                WHERE r.status='CAPTURED' AND e.id IS NULL
                ORDER BY wt.created_at
                LIMIT 200
            ), priced AS (
                SELECT c.*, p.unit_price, p.version pricing_version
                FROM candidates c
                JOIN LATERAL (
                    SELECT unit_price, version
                    FROM payg_pricing_configs
                    WHERE operation=c.operation
                      AND effective_from <= c.created_at
                      AND (effective_until IS NULL OR effective_until > c.created_at)
                      AND unit_price > 0
                    ORDER BY effective_from DESC, version DESC
                    LIMIT 1
                ) p ON TRUE
            )
            INSERT INTO api_usage_events
                (id,tenant_id,operation,units_requested,units_billed,unit_price,
                 charge_units,currency,pricing_version,request_id,status,created_at)
            SELECT gen_random_uuid(), tenant_id, operation,
                   amount_units / unit_price, amount_units / unit_price,
                   unit_price, amount_units, currency, pricing_version,
                   request_id, 'CAPTURED', created_at
            FROM priced
            WHERE amount_units % unit_price = 0
            ON CONFLICT (tenant_id,request_id) DO NOTHING
            RETURNING 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .len() as i64;

        let payg_audit_repaired: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM (
                SELECT wt.id
                FROM credit_reservations r
                JOIN user_wallets w ON w.id=r.wallet_id
                JOIN wallet_transactions wt
                  ON wt.reference_type='credit_reservation'
                 AND wt.reference_id=r.id::text
                 AND wt.transaction_type='DEBIT'
                JOIN api_usage_events e
                  ON e.tenant_id=w.tenant_id AND e.request_id=r.request_id
                CROSS JOIN LATERAL (VALUES ('payg_charged'),('wallet_debited')) a(action)
                WHERE r.status='CAPTURED'
                  AND NOT EXISTS (
                      SELECT 1 FROM financial_audit_log f
                      WHERE f.action=a.action
                        AND f.resource='payg_request'
                        AND f.resource_id=r.request_id
                  )
            ) missing
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        // Insert missing audit rows after counting them. The reservation row is
        // immutable at this point, so the request id is the stable dedupe key.
        sqlx::query(
            r#"
            INSERT INTO financial_audit_log
                (id,actor,tenant_id,action,resource,resource_id,created_at,metadata)
            SELECT gen_random_uuid(), 'system:payg-reconciliation', w.tenant_id,
                   a.action, 'payg_request', r.request_id, now(),
                   jsonb_build_object('operation',r.operation,
                                      'charge_units',-wt.amount_units,
                                      'reconciled',true)
            FROM credit_reservations r
            JOIN user_wallets w ON w.id=r.wallet_id
            JOIN wallet_transactions wt
              ON wt.reference_type='credit_reservation'
             AND wt.reference_id=r.id::text
             AND wt.transaction_type='DEBIT'
            CROSS JOIN LATERAL (VALUES ('payg_charged'),('wallet_debited')) a(action)
            WHERE r.status='CAPTURED'
              AND EXISTS (
                  SELECT 1 FROM api_usage_events e
                  WHERE e.tenant_id=w.tenant_id AND e.request_id=r.request_id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM financial_audit_log f
                  WHERE f.action=a.action
                    AND f.resource='payg_request'
                    AND f.resource_id=r.request_id
              )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // PAYG consistency checks. These are intentionally tenant-scoped and
        // keyed by the durable request id, which is the logical execution
        // identity shared by reservations, usage events and debit ledger rows.
        let execution_without_usage: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM proxy_check_jobs j
            WHERE EXISTS (
                SELECT 1 FROM proxy_entries pe
                WHERE pe.job_id=j.id AND pe.result_json IS NOT NULL
              )
              AND (
                    j.status IN ('completed','failed')
                    OR (j.status='running' AND j.heartbeat_at < now() - interval '5 minutes')
                  )
              AND NOT EXISTS (
                  SELECT 1 FROM api_usage_events e
                  WHERE e.tenant_id=j.tenant_id
                    AND e.request_id='proxy-check:' || j.id::text
              )
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let captured_reservation_without_usage: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM credit_reservations r
            JOIN user_wallets w ON w.id=r.wallet_id
            JOIN wallet_transactions wt
              ON wt.reference_type='credit_reservation'
             AND wt.reference_id=r.id::text
             AND wt.transaction_type='DEBIT'
            LEFT JOIN api_usage_events e
              ON e.tenant_id=w.tenant_id AND e.request_id=r.request_id
            WHERE r.status='CAPTURED' AND e.id IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let chargeable_usage_without_debit: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM api_usage_events e
            LEFT JOIN credit_reservations r
              ON r.request_id=e.request_id
             AND EXISTS (
                 SELECT 1 FROM user_wallets w
                 WHERE w.id=r.wallet_id AND w.tenant_id=e.tenant_id
             )
            LEFT JOIN wallet_transactions wt
              ON wt.reference_type='credit_reservation'
             AND wt.reference_id=r.id::text
             AND wt.transaction_type='DEBIT'
            WHERE e.status='CAPTURED'
              AND e.charge_units > 0
              AND wt.id IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let duplicate_usage_events: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT tenant_id, request_id
                FROM api_usage_events
                GROUP BY tenant_id, request_id
                HAVING COUNT(*) > 1
            ) d
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let duplicate_payg_charges: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM (
                SELECT w.tenant_id, r.request_id
                FROM credit_reservations r
                JOIN user_wallets w ON w.id=r.wallet_id
                JOIN wallet_transactions wt
                  ON wt.reference_type='credit_reservation'
                 AND wt.reference_id=r.id::text
                 AND wt.transaction_type='DEBIT'
                GROUP BY w.tenant_id, r.request_id
                HAVING COUNT(*) > 1
            ) d
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let pending_reservations_past_expiry: i64 = sqlx::query_scalar::<sqlx::Postgres, i64>(
            r#"
            WITH expired AS (
                SELECT id
                FROM credit_reservations
                WHERE status='PENDING' AND expires_at <= now()
                ORDER BY expires_at
                LIMIT 1000
                FOR UPDATE SKIP LOCKED
            )
            UPDATE credit_reservations r
            SET status='EXPIRED', updated_at=now()
            FROM expired
            WHERE r.id=expired.id
            RETURNING 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .len() as i64;

        // Compare the dashboard's charge source of truth (usage events) with
        // PAYG debit ledger totals per tenant. Non-PAYG debit transactions are
        // deliberately excluded.
        let ledger_report_mismatch: i64 = sqlx::query_scalar(
            r#"
            WITH usage AS (
                SELECT tenant_id, COALESCE(SUM(charge_units),0)::BIGINT total
                FROM api_usage_events
                WHERE status='CAPTURED' AND charge_units > 0
                GROUP BY tenant_id
            ),
            ledger AS (
                SELECT w.tenant_id,
                       COALESCE(SUM(-wt.amount_units),0)::BIGINT total
                FROM wallet_transactions wt
                JOIN user_wallets w ON w.id=wt.wallet_id
                JOIN credit_reservations r
                  ON r.id::text=wt.reference_id
                 AND r.wallet_id=w.id
                WHERE wt.transaction_type='DEBIT'
                  AND wt.reference_type='credit_reservation'
                GROUP BY w.tenant_id
            ),
            tenants_seen AS (
                SELECT tenant_id FROM usage
                UNION
                SELECT tenant_id FROM ledger
            )
            SELECT COUNT(*)
            FROM tenants_seen t
            LEFT JOIN usage u ON u.tenant_id=t.tenant_id
            LEFT JOIN ledger l ON l.tenant_id=t.tenant_id
            WHERE COALESCE(u.total,0) <> COALESCE(l.total,0)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        // Keep the existing legacy repair, but only repair immutable financial
        // gaps where a captured debit already proves the charge happened.
        let report = serde_json::json!({
            "provider_source":"signed_billing_webhook_observations",
            "window_start":start,
            "window_end":end,
            "missing_payment":missing_payment,
            "duplicate_payment":duplicate_payment,
            "incorrect_amount":incorrect_amount,
            "orphan_webhook":orphan_webhook,
            "wallet_mismatch":wallet_mismatch,
            "invoice_mismatch":invoice_mismatch,
            "payg_execution_without_usage":execution_without_usage,
            "payg_captured_reservation_without_usage":captured_reservation_without_usage,
            "payg_chargeable_usage_without_debit":chargeable_usage_without_debit,
            "payg_duplicate_usage_events":duplicate_usage_events,
            "payg_duplicate_charges":duplicate_payg_charges,
            "payg_pending_reservations_expired":pending_reservations_past_expiry,
            "payg_ledger_report_mismatch":ledger_report_mismatch,
            "payg_usage_repaired":payg_usage_repaired,
            "payg_audit_missing_before_repair":payg_audit_repaired
        });
        let total = missing_payment
            + duplicate_payment
            + incorrect_amount
            + orphan_webhook
            + wallet_mismatch
            + invoice_mismatch
            + execution_without_usage
            + captured_reservation_without_usage
            + chargeable_usage_without_debit
            + duplicate_usage_events
            + duplicate_payg_charges
            + ledger_report_mismatch;
        for (kind, count) in [
            ("missing_payment", missing_payment),
            ("duplicate_payment", duplicate_payment),
            ("incorrect_amount", incorrect_amount),
            ("orphan_webhook", orphan_webhook),
            ("wallet_mismatch", wallet_mismatch),
            ("invoice_mismatch", invoice_mismatch),
            ("payg_execution_without_usage", execution_without_usage),
            (
                "payg_captured_reservation_without_usage",
                captured_reservation_without_usage,
            ),
            (
                "payg_chargeable_usage_without_debit",
                chargeable_usage_without_debit,
            ),
            ("payg_duplicate_usage_events", duplicate_usage_events),
            ("payg_duplicate_charges", duplicate_payg_charges),
            (
                "payg_pending_reservations_expired",
                pending_reservations_past_expiry,
            ),
            ("payg_ledger_report_mismatch", ledger_report_mismatch),
        ] {
            if count > 0 {
                metrics::counter!("billing_reconciliation_mismatches_total", "kind" => kind)
                    .increment(count as u64);
            }
        }
        metrics::gauge!("billing_reconciliation_last_status").set(if total == 0 {
            0.0
        } else {
            1.0
        });
        let status = if total == 0 { "ok" } else { "mismatch" };
        sqlx::query("INSERT INTO billing_reconciliation_reports(id,run_started_at,run_finished_at,window_start,window_end,status,mismatch_counts,report) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
   .bind(Uuid::new_v4()).bind(started).bind(Utc::now()).bind(start).bind(end).bind(status)
   .bind(serde_json::json!({
       "missing_payment":missing_payment,
       "duplicate_payment":duplicate_payment,
       "incorrect_amount":incorrect_amount,
       "orphan_webhook":orphan_webhook,
       "wallet_mismatch":wallet_mismatch,
       "invoice_mismatch":invoice_mismatch,
       "payg_execution_without_usage":execution_without_usage,
       "payg_captured_reservation_without_usage":captured_reservation_without_usage,
       "payg_chargeable_usage_without_debit":chargeable_usage_without_debit,
       "payg_duplicate_usage_events":duplicate_usage_events,
       "payg_duplicate_charges":duplicate_payg_charges,
       "payg_pending_reservations_expired":pending_reservations_past_expiry,
       "payg_ledger_report_mismatch":ledger_report_mismatch
   }))
   .bind(&report).execute(&self.pool).await?;
        if total > 0 {
            tracing::warn!(
                missing_payment,
                duplicate_payment,
                incorrect_amount,
                orphan_webhook,
                wallet_mismatch,
                invoice_mismatch,
                execution_without_usage,
                captured_reservation_without_usage,
                chargeable_usage_without_debit,
                duplicate_usage_events,
                duplicate_payg_charges,
                pending_reservations_past_expiry,
                ledger_report_mismatch,
                "billing reconciliation mismatch detected"
            );
        } else {
            info!("billing reconciliation OK");
        }
        Ok(report)
    }
}
PINGVERTEX_EOF

if [[ -f "crates/worker/src/main.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/src/main.rs")"
  cp "crates/worker/src/main.rs" "$BACKUP_DIR/crates/worker/src/main.rs"
fi

mkdir -p "$(dirname "crates/worker/src/main.rs")"
cat > "crates/worker/src/main.rs" <<'PINGVERTEX_EOF'
mod config;

use std::{error::Error, sync::Arc, time::Duration};

use application::ports::{PaymentProvider, StatusPageCache};
use async_trait::async_trait;
use infra::{
    AesGcmProxyCredentialCipher, BillingConfig, DisabledBillingProvider, FakeBillingProvider,
    HickoryCustomDomainVerifier, NotificationDispatcher, PostgresApiFlowRepository,
    PostgresCheckRepository, PostgresDiagnosticReportRepository,
    PostgresEscalationPolicyRepository, PostgresHistoryRetentionRepository,
    PostgresIncidentRepository, PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresNotificationOutboxRepository,
    PostgresPAYGPricingRepository, PostgresProxyCheckJobRepository, PostgresSchedulingRepository,
    PostgresStatusPageRepository, PostgresSubscriptionRepository, PostgresTenantRepository,
    PostgresUsageEventRepository, RedisStatusPageCache, UuidIdGenerator, create_pool,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;
use worker::{
    AutoTopUpWorker, CustomDomainVerificationWorker, EscalationWorker, HeartbeatSweeper,
    IncidentEvaluator, IncidentManager, MultiCheckExecutor, OutboxWorker, ProxyCheckWorker,
    RealNotificationOutboxSender, ReconciliationWorker, RetentionWorker,
    TenantNotificationRateLimiter, WalletExpirationWorker, WalletNotificationEmailWorker,
    init_rustls_crypto_provider, metrics,
};

#[derive(Clone, Default)]
struct NoopStatusPageCache;

#[async_trait]
impl StatusPageCache for NoopStatusPageCache {
    async fn get(&self, _key: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }

    async fn set(
        &self,
        _key: &str,
        _value: &str,
        _ttl_seconds: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }

    async fn invalidate(&self, _key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_rustls_crypto_provider();

    // P0 hardening: refuse to boot in production if the private-IP SSRF
    // escape hatch is enabled.
    shared::ssrf::validate_production_ssrf_config()?;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=trace"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    metrics::init_metrics();

    tokio::spawn(async {
        if let Err(err) = metrics::serve_metrics().await {
            tracing::error!(error = %err, "worker metrics server failed");
        }
    });

    let worker_config = config::WorkerConfig::from_env();
    let pool = create_pool(&worker_config.database_url).await?;

    let cancel_token = CancellationToken::new();
    let billing_config = BillingConfig::try_from_env()?;
    let billing_provider: Arc<dyn PaymentProvider> = match billing_config.provider.as_str() {
        "fake" => Arc::new(FakeBillingProvider),
        "disabled" => Arc::new(DisabledBillingProvider),
        other => return Err(format!("unsupported billing provider: {other}").into()),
    };

    let monitor_repository = Arc::new(PostgresMonitorRepository::new(pool.clone()));
    let check_repository = Arc::new(PostgresCheckRepository::new(pool.clone()));
    let incident_repository = Arc::new(PostgresIncidentRepository::new(pool.clone()));
    let notification_channel_repository =
        Arc::new(PostgresNotificationChannelRepository::new(pool.clone()));
    let maintenance_window_repository =
        Arc::new(PostgresMaintenanceWindowRepository::new(pool.clone()));
    let escalation_policy_repository =
        Arc::new(PostgresEscalationPolicyRepository::new(pool.clone()));
    let _diagnostic_report_repository =
        Arc::new(PostgresDiagnosticReportRepository::new(pool.clone()));
    let status_page_repository = Arc::new(PostgresStatusPageRepository::new(pool.clone()));
    let custom_domain_verifier = Arc::new(HickoryCustomDomainVerifier::new());
    let id_generator = Arc::new(UuidIdGenerator::new());

    let tenant_repository = Arc::new(PostgresTenantRepository::new(pool.clone()));
    let subscription_repository = Arc::new(PostgresSubscriptionRepository::new(pool.clone()));
    let wallet_repository = Arc::new(infra::PostgresWalletRepository::new(pool.clone()));
    let financial_audit_repository = Arc::new(infra::PostgresFinancialAuditLogRepository::new(
        pool.clone(),
    ));
    let payg_pricing_repository = Arc::new(PostgresPAYGPricingRepository::new(pool.clone()));
    let usage_event_repository = Arc::new(PostgresUsageEventRepository::new(pool.clone()));
    let metering = Arc::new(
        application::MeterUsageUseCase::new(
            wallet_repository.clone(),
            payg_pricing_repository,
            usage_event_repository,
        )
        .with_audit_log_repository(financial_audit_repository.clone()),
    );
    let auto_top_up_worker = AutoTopUpWorker::new(
        wallet_repository.clone(),
        billing_provider,
        Duration::from_secs(300),
    )
    .with_audit_repository(financial_audit_repository.clone());
    let auto_top_up_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = auto_top_up_worker.run_forever(auto_top_up_cancel).await {
            tracing::error!(error=%err,"auto top-up worker stopped unexpectedly");
        }
    });

    let history_retention_repository =
        Arc::new(PostgresHistoryRetentionRepository::new(pool.clone()));

    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let proxy_check_job_repository = Arc::new(PostgresProxyCheckJobRepository::new(
        pool.clone(),
        proxy_credential_cipher,
    ));

    let secret_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let notification_service = Arc::new(NotificationDispatcher::from_env_with_metering(
        secret_cipher,
        metering.clone(),
    ));
    let wallet_email_worker = WalletNotificationEmailWorker::new(
        wallet_repository.clone(),
        notification_channel_repository.clone(),
        notification_service.clone(),
        Duration::from_secs(60),
    );
    let wallet_email_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = wallet_email_worker.run_forever(wallet_email_cancel).await {
            tracing::error!(error=%err, "wallet notification email worker stopped unexpectedly");
        }
    });
    let api_flow_repository = Arc::new(PostgresApiFlowRepository::new(pool.clone()));

    let notification_outbox_repository =
        Arc::new(PostgresNotificationOutboxRepository::new(pool.clone()));

    let status_page_cache: Arc<dyn StatusPageCache> = match &worker_config.redis_url {
        Some(redis_url) => Arc::new(RedisStatusPageCache::new(redis_url)?),
        None => {
            tracing::warn!(
                "REDIS_URL not configured — incident open/resolve will not invalidate cached \
                 public status pages immediately (falls back to TTL-only staleness)"
            );
            Arc::new(NoopStatusPageCache)
        }
    };

    let incident_manager = IncidentManager::new_with_maintenance_window(
        incident_repository.clone(),
        id_generator.clone(),
        notification_channel_repository.clone(),
        notification_service.clone(),
        maintenance_window_repository.clone(),
    )
    .with_status_page_cache_invalidation(status_page_repository.clone(), status_page_cache)
    .with_notification_outbox(notification_outbox_repository.clone());

    let incident_evaluator = IncidentEvaluator::new(
        monitor_repository.clone(),
        incident_manager,
        worker_config.incident_failure_threshold,
        worker_config.incident_recovery_threshold,
    );

    let escalation_worker = EscalationWorker::new(
        incident_repository.clone(),
        monitor_repository.clone(),
        escalation_policy_repository,
        notification_channel_repository.clone(),
        notification_service.clone(),
    );
    let escalation_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = escalation_worker.run_forever(escalation_cancel).await {
            tracing::error!(error = %err, "escalation worker stopped unexpectedly");
        }
    });

    let custom_domain_verification_worker =
        CustomDomainVerificationWorker::new(status_page_repository, custom_domain_verifier);
    let custom_domain_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = custom_domain_verification_worker
            .run_forever(custom_domain_cancel)
            .await
        {
            tracing::error!(
                error = %err,
                "custom domain verification worker stopped unexpectedly"
            );
        }
    });

    let proxy_check_worker = ProxyCheckWorker::new(proxy_check_job_repository, metering.clone());
    let proxy_check_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = proxy_check_worker.run_forever(proxy_check_cancel).await {
            tracing::error!(
                error = %err,
                "proxy check worker stopped unexpectedly"
            );
        }
    });

    let reconciliation_worker =
        ReconciliationWorker::new(pool.clone(), Duration::from_secs(24 * 60 * 60));
    let reconciliation_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = reconciliation_worker
            .run_forever(reconciliation_cancel)
            .await
        {
            tracing::error!(error=%err, "billing reconciliation worker stopped unexpectedly");
        }
    });

    let wallet_expiration_worker =
        WalletExpirationWorker::new(wallet_repository, Duration::from_secs(300), 200);
    let wallet_expiration_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = wallet_expiration_worker
            .run_forever(wallet_expiration_cancel)
            .await
        {
            tracing::error!(error=%err, "wallet expiration worker stopped unexpectedly");
        }
    });

    let retention_worker = RetentionWorker::new(
        tenant_repository,
        subscription_repository,
        history_retention_repository,
        Arc::new(PostgresPricingConfigRepository::new(pool.clone())),
        worker::retention_worker::DEFAULT_BATCH_SIZE,
        worker::retention_worker::DEFAULT_RETENTION_INTERVAL,
        worker_config.retention_max_concurrent_tenants,
    );
    let retention_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = retention_worker.run_forever(retention_cancel).await {
            tracing::error!(
                error = %err,
                "retention worker stopped unexpectedly"
            );
        }
    });

    let heartbeat_sweeper = HeartbeatSweeper::new(monitor_repository.clone());
    let heartbeat_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = heartbeat_sweeper
            .run_forever(
                Duration::from_secs(worker_config.lease_seconds / 2),
                heartbeat_cancel,
            )
            .await
        {
            tracing::error!(
                error = %err,
                "heartbeat sweeper stopped unexpectedly"
            );
        }
    });

    let scheduling_repository = Arc::new(PostgresSchedulingRepository::new(pool.clone()));
    let executor = Arc::new(MultiCheckExecutor::new(
        check_repository.clone(),
        id_generator.clone(),
        incident_evaluator,
        api_flow_repository,
    ));

    let tenant_notification_rate_limiter = Arc::new(TenantNotificationRateLimiter::new(
        60,
        Duration::from_secs(60),
    ));
    let notification_outbox_sender = Arc::new(RealNotificationOutboxSender::new(
        notification_service.clone(),
        notification_channel_repository.clone(),
        tenant_notification_rate_limiter,
    ));
    let notification_outbox_worker =
        OutboxWorker::new(notification_outbox_repository, notification_outbox_sender);
    let outbox_cancel = cancel_token.clone();
    tokio::spawn(async move {
        if let Err(err) = notification_outbox_worker.run_forever(outbox_cancel).await {
            tracing::error!(
                error = %err,
                "notification outbox worker stopped unexpectedly"
            );
        }
    });

    tracing::info!("worker started; press Ctrl+C or send SIGTERM to shut down");

    tokio::select! {
        result = worker::DurableScheduler::new(
            scheduling_repository,
            executor,
            Duration::from_secs(worker_config.lease_seconds),
            worker_config.max_concurrent_checks,
        )
        .with_metering(metering.clone())
        .run() => {
            result?;
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received, cancelling background workers");
        }
    }

    cancel_token.cancel();
    tracing::info!(
        "cancellation token triggered; waiting briefly for background workers to finish"
    );
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    tracing::info!("worker shut down");

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
PINGVERTEX_EOF

if [[ -f "crates/api/src/error.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/api/src/error.rs")"
  cp "crates/api/src/error.rs" "$BACKUP_DIR/crates/api/src/error.rs"
fi

mkdir -p "$(dirname "crates/api/src/error.rs")"
cat > "crates/api/src/error.rs" <<'PINGVERTEX_EOF'
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

/// Unified API error.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Forbidden(String),
    Unauthorized(String),
    TooManyRequests(String),
    PaymentRequired { message: String, code: &'static str },
    Internal(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// 403 Forbidden — the caller is authenticated but their role lacks
    /// the permission required for this action (e.g. `Role::Auditor`
    /// calling an endpoint gated on `Permission::ManageApiKeys`).
    /// Distinct from `UNAUTHORIZED`, which means "not authenticated at
    /// all" and is handled upstream by the auth middleware.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// 401 Unauthorized — used for auth-flow failures that are NOT about
    /// missing/invalid session tokens (that case is handled upstream by
    /// `jwt_auth` returning a bare `StatusCode`, before an `ApiError` is
    /// ever constructed). This variant covers e.g. an invalid/expired
    /// pending 2FA token, an invalid TOTP/backup code, or a failed
    /// password step-up re-authentication check.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// 429 Too Many Requests — used when a caller-specific quota (not the
    /// IP-based `GovernorLayer` rate limiter, which axum never sees as an
    /// `ApiError`) has been exceeded, e.g. the daily proxy-check job quota.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::TooManyRequests(message.into())
    }

    /// Generic internal error.
    ///
    /// ONLY use this when `message` is already known to be safe to show a
    /// client (a static, hand-written string with no interpolated
    /// database/driver/library error text). If you are wrapping an
    /// underlying `Error`, use [`ApiError::internal_with_source`] instead
    /// — that constructor logs the real cause and returns a generic
    /// message to the caller.
    pub fn payment_required(message: impl Into<String>, code: &'static str) -> Self {
        Self::PaymentRequired {
            message: message.into(),
            code,
        }
    }

    pub fn from_payg_error(err: application::ChargeForUsageError) -> Self {
        match err {
            application::ChargeForUsageError::InsufficientBalance => {
                Self::payment_required("PAYG balance is insufficient", "PAYG_INSUFFICIENT_BALANCE")
            }
            application::ChargeForUsageError::InvalidInput(message) => Self::bad_request(message),
            application::ChargeForUsageError::ReservationInProgress => {
                Self::conflict("PAYG request is already in progress")
            }
            application::ChargeForUsageError::OperationFailed(message) => {
                Self::internal_with_source("metered operation failed", message)
            }
            application::ChargeForUsageError::OperationNotAttempted(message) => {
                Self::bad_request(message)
            }
            application::ChargeForUsageError::NoPricing
            | application::ChargeForUsageError::InvalidPricing(_) => {
                Self::internal("PAYG pricing is unavailable")
            }
            application::ChargeForUsageError::Repository(err)
            | application::ChargeForUsageError::CaptureFailed(err) => {
                Self::internal_with_source("PAYG operation failed", err)
            }
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Preferred constructor for internal errors that wrap an underlying
    /// cause (database errors, I/O errors, etc).
    ///
    /// The full error (`err`) is logged server-side via `tracing::error!`
    /// together with `context`, so operators retain complete diagnostic
    /// information. The HTTP response sent to the client contains ONLY
    /// the generic `context` string — never the raw `{err}` — so
    /// internal details such as table/column names or database driver
    /// messages never leak to an untrusted caller. This closes an
    /// information-disclosure gap identified in the 2026-08-11 security
    /// audit, where 500 responses previously echoed raw backend errors.
    pub fn internal_with_source(context: &str, err: impl std::fmt::Display) -> Self {
        error!(error = %err, "{context}");
        Self::Internal(context.to_string())
    }

    /// Maps a use-case error to `403 Forbidden` when it is (or wraps) a
    /// [`domain::shared::types::PermissionDenied`], otherwise falls back to
    /// [`ApiError::internal_with_source`].
    ///
    /// Use cases enforce authorization by calling `AuthenticatedUser::require`,
    /// which returns `PermissionDenied` on a boxed `dyn Error`. Handlers must
    /// route that through here (or an equivalent explicit check) rather than
    /// letting it fall into a generic 500 — a permission failure is a client
    /// error, and treating it as an opaque internal error also makes such
    /// bypasses harder to notice in logs/monitoring.
    pub fn forbidden_or_internal(
        context: &str,
        err: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        if let Some(denied) = err.downcast_ref::<domain::shared::types::PermissionDenied>() {
            return Self::forbidden(denied.to_string());
        }
        Self::internal_with_source(context, err)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::PaymentRequired { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::Forbidden(message)
            | Self::Unauthorized(message)
            | Self::TooManyRequests(message)
            | Self::Internal(message) => message.clone(),
            Self::PaymentRequired { message, .. } => message.clone(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = match &self {
            ApiError::PaymentRequired { code, .. } => Some(*code),
            _ => None,
        };
        let body = Json(ErrorBody {
            message: self.message(),
            code,
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Misalin "kuskure" mai kama da wanda sqlx/postgres ke iya bayarwa —
    /// yana ƙunshe da sunan table/column, wanda BA A SO ya bayyana ga
    /// client.
    struct FakeDbError(&'static str);

    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[test]
    fn internal_with_source_never_leaks_raw_error_text_to_client_message() {
        let sensitive =
            FakeDbError("duplicate key value violates unique constraint \"monitors_url_idx\"");

        let err = ApiError::internal_with_source("failed to create monitor", sensitive);

        match &err {
            ApiError::Internal(message) => {
                assert_eq!(message, "failed to create monitor");
                assert!(
                    !message.contains("unique constraint"),
                    "client-facing message must not contain raw DB error text, got: {message}"
                );
                assert!(
                    !message.contains("monitors_url_idx"),
                    "client-facing message must not contain table/column names, got: {message}"
                );
            }
            other => panic!("expected ApiError::Internal, got {other:?}"),
        }
    }

    #[test]
    fn internal_with_source_response_body_is_generic() {
        let sensitive = FakeDbError("relation \"monitors\" does not exist at line 42");

        let err = ApiError::internal_with_source("failed to fetch monitor", sensitive);
        let status = err.status_code();
        let message = err.message();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message, "failed to fetch monitor");
        assert!(!message.contains("relation"));
        assert!(!message.contains("line 42"));
    }

    #[test]
    fn internal_static_message_is_used_verbatim() {
        let err = ApiError::internal("failed to set cookie");
        assert_eq!(err.message(), "failed to set cookie");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn bad_request_and_not_found_still_carry_their_own_message() {
        assert_eq!(
            ApiError::bad_request("invalid uuid").message(),
            "invalid uuid"
        );
        assert_eq!(
            ApiError::not_found("monitor not found").message(),
            "monitor not found"
        );
        assert_eq!(
            ApiError::bad_request("x").status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::not_found("x").status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn forbidden_carries_its_own_message_and_status() {
        assert_eq!(
            ApiError::forbidden("you do not have permission to do this").message(),
            "you do not have permission to do this"
        );
        assert_eq!(
            ApiError::forbidden("x").status_code(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn payment_required_uses_402_and_application_code() {
        let err =
            ApiError::payment_required("PAYG balance is insufficient", "PAYG_INSUFFICIENT_BALANCE");
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(err.message(), "PAYG balance is insufficient");
    }

    #[test]
    fn too_many_requests_carries_its_own_message() {
        assert_eq!(
            ApiError::too_many_requests("daily limit reached").message(),
            "daily limit reached"
        );
        assert_eq!(
            ApiError::too_many_requests("x").status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
PINGVERTEX_EOF

if [[ -f "apps/leptos-dashboard/src/pages/usage.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "apps/leptos-dashboard/src/pages/usage.rs")"
  cp "apps/leptos-dashboard/src/pages/usage.rs" "$BACKUP_DIR/apps/leptos-dashboard/src/pages/usage.rs"
fi

mkdir -p "$(dirname "apps/leptos-dashboard/src/pages/usage.rs")"
cat > "apps/leptos-dashboard/src/pages/usage.rs" <<'PINGVERTEX_EOF'
use crate::api::wallet;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(u: i64) -> String {
    let sign = if u < 0 { "-" } else { "" };
    let a = u.unsigned_abs();

    format!("{sign}${}.{:02}", a / 1_000_000, (a % 1_000_000) / 10_000)
}

fn date_start(v: &str) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!("{v}T00:00:00Z")
    }
}

fn date_end(v: &str) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!("{v}T23:59:59Z")
    }
}

fn chart_width(value: i64, maximum: i64) -> String {
    if maximum <= 0 || value <= 0 {
        "0%".to_owned()
    } else {
        let width = ((value as f64 / maximum as f64) * 100.0).round() as i64;
        format!("{}%", width.clamp(2, 100))
    }
}

#[component]
pub fn UsagePage() -> impl IntoView {
    let report = RwSignal::new(None::<wallet::UsageReport>);
    let error = RwSignal::new(None::<String>);

    let from = RwSignal::new(String::new());
    let to = RwSignal::new(String::new());
    let operation = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());

    let reload = RwSignal::new(0_u32);

    Effect::new(move |_| {
        let _ = reload.get();

        let f = date_start(&from.get());
        let t = date_end(&to.get());
        let op = operation.get();
        let st = status.get();

        spawn_local(async move {
            match wallet::usage(
                (!f.is_empty()).then_some(f.as_str()),
                (!t.is_empty()).then_some(t.as_str()),
                (!op.is_empty()).then_some(op.as_str()),
                (!st.is_empty()).then_some(st.as_str()),
            )
            .await
            {
                Ok(value) => {
                    error.set(None);
                    report.set(Some(value));
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
        });
    });

    let apply_filters = move |_| {
        reload.update(|value| *value += 1);
    };

    let clear_filters = move |_| {
        from.set(String::new());
        to.set(String::new());
        operation.set(String::new());
        status.set(String::new());
        reload.update(|value| *value += 1);
    };

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Usage"</h1>
                    <p class="muted">
                        "Metered usage, spend and projected monthly cost."
                    </p>
                </div>
            </header>

            <div class="notice" role="note">
                "Monitoring attempts are billed when a check runs, including failed target checks or network errors."
            </div>

            {move || {
                error.get().map(|message| {
                    view! {
                        <div class="error" role="alert">
                            {message}
                        </div>
                    }
                })
            }}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Filters"</h2>
                        <p class="muted">
                            "Narrow the report without changing the underlying ledger."
                        </p>
                    </div>
                </div>

                <div class="inline-form">
                    <label>
                        "From "
                        <input
                            type="date"
                            prop:value=move || from.get()
                            on:input=move |event| {
                                from.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <label>
                        "To "
                        <input
                            type="date"
                            prop:value=move || to.get()
                            on:input=move |event| {
                                to.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <label>
                        "Operation "
                        <input
                            placeholder="proxy_check"
                            maxlength="100"
                            prop:value=move || operation.get()
                            on:input=move |event| {
                                operation.set(event_target_value(&event));
                            }
                        />
                    </label>

                    <label>
                        "Status "
                        <select
                            prop:value=move || status.get()
                            on:change=move |event| {
                                status.set(event_target_value(&event));
                            }
                        >
                            <option value="">"All"</option>
                            <option value="SUCCEEDED">"Succeeded"</option>
                            <option value="FAILED">"Failed"</option>
                        </select>
                    </label>

                    <div class="inline-actions">
                        <button
                            type="button"
                            class="btn btn-primary"
                            on:click=apply_filters
                        >
                            "Apply"
                        </button>

                        <button
                            type="button"
                            class="btn"
                            on:click=clear_filters
                        >
                            "Clear"
                        </button>
                    </div>
                </div>
            </section>

            {move || {
                if let Some(r) = report.get() {
                    view! {
                        <section class="metric-grid">
                            <article class="card metric-card">
                                <span class="eyebrow">"CURRENT BALANCE"</span>
                                <strong>{usd(r.current_balance_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"TODAY'S USAGE"</span>
                                <strong>{usd(r.today_usage_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"THIS MONTH"</span>
                                <strong>{usd(r.month_usage_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"MONTH SPEND"</span>
                                <strong>{usd(r.month_spend_units)}</strong>
                            </article>

                            <article class="card metric-card">
                                <span class="eyebrow">"PROJECTED SPEND"</span>
                                <strong>{usd(r.projected_spend_units)}</strong>
                            </article>
                        </section>
                    }
                    .into_any()
                } else {
                    view! {
                        <section class="card">
                            <div class="loading">
                                "Loading usage report…"
                            </div>
                        </section>
                    }
                    .into_any()
                }
            }}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Runtime metering"</h2>
                        <p class="muted">"PAYG charges now cover the live runtime paths below."</p>
                    </div>
                </div>
                <div class="overview-cards">
                    <div><span class="card-label">"Monitor checks"</span><span class="card-value">"monitor_check"</span></div>
                    <div><span class="card-label">"Proxy checks"</span><span class="card-value">"proxy_check"</span></div>
                    <div><span class="card-label">"SMS"</span><span class="card-value">"sms"</span></div>
                    <div><span class="card-label">"RUM"</span><span class="card-value">"rum_event"</span></div>
                    <div><span class="card-label">"Storage"</span><span class="card-value">"storage"</span></div>
                    <div><span class="card-label">"Heartbeat"</span><span class="card-value">"heartbeat_ping"</span></div>
                </div>
                <p class="muted">"Monitor checks are billed once per actual billable attempt. Target failures, timeouts, DNS failures, and connection refusals are billable when the external attempt started; validation and internal pre-attempt failures are not billed. Proxy jobs are metered per proxy entry."</p>
            </section>

            {move || {
                if let Some(r) = report.get() {
                    if r.daily.is_empty() {
                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Usage"</h2>
                                        <p class="muted">
                                            "Usage units grouped by day."
                                        </p>
                                    </div>
                                </div>
                                <div class="empty-state">
                                    <h3>"No daily usage"</h3>
                                    <p>"No usage matched the selected filters."</p>
                                </div>
                            </section>
                        }
                        .into_any()
                    } else {
                        let max_usage = r
                            .daily
                            .iter()
                            .map(|item| item.usage_units)
                            .max()
                            .unwrap_or(0);

                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Usage"</h2>
                                        <p class="muted">
                                            "Usage units grouped by day."
                                        </p>
                                    </div>
                                </div>

                                <div class="usage-chart" role="img" aria-label="Daily usage chart">
                                    {r.daily
                                        .iter()
                                        .map(|item| {
                                            let width = chart_width(item.usage_units, max_usage);

                                            view! {
                                                <div class="usage-chart-row">
                                                    <div class="usage-chart-label">
                                                        {item.day.clone()}
                                                    </div>

                                                    <div class="usage-chart-track">
                                                        <div
                                                            class="usage-chart-bar"
                                                            style=move || format!("width: {width}")
                                                            title=format!(
                                                                "{}: {}",
                                                                item.day,
                                                                usd(item.usage_units)
                                                            )
                                                        ></div>
                                                    </div>

                                                    <div class="usage-chart-value">
                                                        {usd(item.usage_units)}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                        .into_any()
                    }
                } else {
                    ().into_any()
                }
            }}

            {move || {
                if let Some(r) = report.get() {
                    if r.daily.is_empty() {
                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Spend"</h2>
                                        <p class="muted">
                                            "PAYG spend grouped by day."
                                        </p>
                                    </div>
                                </div>
                                <div class="empty-state">
                                    <h3>"No daily spend"</h3>
                                    <p>"No spend matched the selected filters."</p>
                                </div>
                            </section>
                        }
                        .into_any()
                    } else {
                        let max_spend = r
                            .daily
                            .iter()
                            .map(|item| item.spend_units)
                            .max()
                            .unwrap_or(0);

                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Daily Spend"</h2>
                                        <p class="muted">
                                            "PAYG spend grouped by day."
                                        </p>
                                    </div>
                                </div>

                                <div class="usage-chart" role="img" aria-label="Daily spend chart">
                                    {r.daily
                                        .iter()
                                        .map(|item| {
                                            let width = chart_width(item.spend_units, max_spend);

                                            view! {
                                                <div class="usage-chart-row">
                                                    <div class="usage-chart-label">
                                                        {item.day.clone()}
                                                    </div>

                                                    <div class="usage-chart-track">
                                                        <div
                                                            class="usage-chart-bar"
                                                            style=move || format!("width: {width}")
                                                            title=format!(
                                                                "{}: {}",
                                                                item.day,
                                                                usd(item.spend_units)
                                                            )
                                                        ></div>
                                                    </div>

                                                    <div class="usage-chart-value">
                                                        {usd(item.spend_units)}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                        .into_any()
                    }
                } else {
                    ().into_any()
                }
            }}

            {move || {
                if let Some(r) = report.get() {
                    if r.by_operation.is_empty() {
                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Usage by Operation"</h2>
                                        <p class="muted">
                                            "Usage grouped by metered operation."
                                        </p>
                                    </div>
                                </div>

                                <div class="empty-state">
                                    <h3>"No usage"</h3>
                                    <p>"No usage matched the selected filters."</p>
                                </div>
                            </section>
                        }
                        .into_any()
                    } else {
                        let max_usage = r
                            .by_operation
                            .iter()
                            .map(|item| item.usage_units)
                            .max()
                            .unwrap_or(0);

                        view! {
                            <section class="card">
                                <div class="section-heading">
                                    <div>
                                        <h2>"Usage by Operation"</h2>
                                        <p class="muted">
                                            "Usage and spend grouped by operation."
                                        </p>
                                    </div>
                                </div>

                                <div class="usage-chart" role="img" aria-label="Usage by operation chart">
                                    {r.by_operation
                                        .iter()
                                        .map(|item| {
                                            let width =
                                                chart_width(item.usage_units, max_usage);

                                            view! {
                                                <div class="usage-chart-row">
                                                    <div class="usage-chart-label">
                                                        {item.operation.clone()}
                                                    </div>

                                                    <div class="usage-chart-track">
                                                        <div
                                                            class="usage-chart-bar"
                                                            style=move || format!("width: {width}")
                                                            title=format!(
                                                                "{}: {} usage · {} spend",
                                                                item.operation,
                                                                usd(item.usage_units),
                                                                usd(item.spend_units)
                                                            )
                                                        ></div>
                                                    </div>

                                                    <div class="usage-chart-value">
                                                        {usd(item.spend_units)}
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </section>
                        }
                        .into_any()
                    }
                } else {
                    ().into_any()
                }
            }}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Usage Ledger"</h2>
                        <p class="muted">
                            "Individual PAYG usage records returned by the server."
                        </p>
                    </div>
                </div>

                {move || {
                    if let Some(r) = report.get() {
                        if r.rows.is_empty() {
                            view! {
                                <div class="empty-state">
                                    <h3>"No usage records"</h3>
                                    <p>
                                        "No usage matched the selected filters."
                                    </p>
                                </div>
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="table-shell">
                                    <table class="data-table">
                                        <thead>
                                            <tr>
                                                <th>"Timestamp"</th>
                                                <th>"Operation"</th>
                                                <th>"Units"</th>
                                                <th>"Price"</th>
                                                <th>"Charge"</th>
                                                <th>"Request ID"</th>
                                                <th>"Status"</th>
                                            </tr>
                                        </thead>

                                        <tbody>
                                            {r.rows
                                                .iter()
                                                .map(|row| {
                                                    view! {
                                                        <tr>
                                                            <td>{row.timestamp.clone()}</td>
                                                            <td><code>{row.operation.clone()}</code></td>
                                                            <td>{usd(row.units)}</td>
                                                            <td>{usd(row.price_units)}</td>
                                                            <td>{usd(row.charge_units)}</td>
                                                            <td>
                                                                <code>{row.request_id.clone()}</code>
                                                            </td>
                                                            <td>
                                                                <span class="status-badge">
                                                                    {row.status.clone()}
                                                                </span>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                </div>
                            }
                            .into_any()
                        }
                    } else {
                        view! {
                            <div class="loading">
                                "Loading usage ledger…"
                            </div>
                        }
                        .into_any()
                    }
                }}
            </section>

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Usage Summary"</h2>
                        <p class="muted">
                            "Financial totals are calculated server-side from the PAYG ledger."
                        </p>
                    </div>
                </div>

                {move || {
                    if let Some(r) = report.get() {
                        view! {
                            <div class="list-stack">
                                <div class="list-row">
                                    <span>"Current balance"</span>
                                    <strong>{usd(r.current_balance_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Today's usage"</span>
                                    <strong>{usd(r.today_usage_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Month usage"</span>
                                    <strong>{usd(r.month_usage_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Month spend"</span>
                                    <strong>{usd(r.month_spend_units)}</strong>
                                </div>

                                <div class="list-row">
                                    <span>"Projected spend"</span>
                                    <strong>{usd(r.projected_spend_units)}</strong>
                                </div>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div class="muted">"Loading…"</div> }.into_any()
                    }
                }}
            </section>
        </div>
    }
}
PINGVERTEX_EOF

if [[ -f "crates/application/tests/resolve_entitlements_use_case.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/application/tests/resolve_entitlements_use_case.rs")"
  cp "crates/application/tests/resolve_entitlements_use_case.rs" "$BACKUP_DIR/crates/application/tests/resolve_entitlements_use_case.rs"
fi

mkdir -p "$(dirname "crates/application/tests/resolve_entitlements_use_case.rs")"
cat > "crates/application/tests/resolve_entitlements_use_case.rs" <<'PINGVERTEX_EOF'
use application::ResolveEntitlementsUseCase;
use application::ports::SubscriptionRepository;
use application::queries::ResolveEntitlements;
use domain::Subscription;
use domain::enums::{PlanTier, SubscriptionStatus};
use domain::shared::ids::{SubscriptionId, TenantId};
use infra::InMemorySubscriptionRepository;

#[tokio::test]
async fn defaults_to_free_plan_when_no_subscription_exists() {
    let use_case = ResolveEntitlementsUseCase::new(InMemorySubscriptionRepository::new());
    let tenant_id = TenantId::new();

    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
    assert_eq!(plan.max_monitors, 20);
    assert_eq!(plan.min_check_interval_seconds, 300);
    assert_eq!(plan.history_retention_days, 2);
}

#[tokio::test]
async fn resolves_correct_limits_for_each_paid_tier() {
    let cases = [
        (PlanTier::Starter, 25, 60, 30),
        (PlanTier::Pro, 100, 30, 90),
        (PlanTier::Business, 500, 15, 365),
        (PlanTier::Enterprise, 10_000, 15, 3650),
    ];

    for (tier, max_monitors, min_interval, retention_days) in cases {
        let repository = InMemorySubscriptionRepository::new();
        let tenant_id = TenantId::new();
        let subscription = Subscription::new(SubscriptionId::new(), tenant_id, tier);
        repository.create(&subscription).await.unwrap();

        let use_case = ResolveEntitlementsUseCase::new(repository);

        let plan = use_case
            .execute(ResolveEntitlements { tenant_id })
            .await
            .unwrap();

        assert_eq!(plan.tier, tier);
        assert_eq!(plan.max_monitors, max_monitors);
        assert_eq!(plan.min_check_interval_seconds, min_interval);
        assert_eq!(plan.history_retention_days, retention_days);
    }
}

#[tokio::test]
async fn trialing_subscription_resolves_full_plan_entitlements() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let subscription = Subscription::new_trialing(
        SubscriptionId::new(),
        tenant_id,
        PlanTier::Business,
        chrono::Utc::now(),
    );
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Business);
}

#[tokio::test]
async fn past_due_subscription_still_resolves_full_plan_entitlements() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Pro);
    subscription.status = SubscriptionStatus::PastDue;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Pro);
}

#[tokio::test]
async fn expired_subscription_falls_back_to_free_plan() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription =
        Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Enterprise);
    subscription.status = SubscriptionStatus::Expired;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
}

#[tokio::test]
async fn canceled_subscription_falls_back_to_free_plan() {
    let repository = InMemorySubscriptionRepository::new();
    let tenant_id = TenantId::new();
    let mut subscription =
        Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Enterprise);
    subscription.status = SubscriptionStatus::Canceled;
    repository.create(&subscription).await.unwrap();

    let use_case = ResolveEntitlementsUseCase::new(repository);
    let plan = use_case
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.tier, PlanTier::Free);
}

#[tokio::test]
async fn free_entitlements_follow_active_database_pricing_config() {
    let subscription_repository = InMemorySubscriptionRepository::new();
    let pricing_repository = infra::InMemoryPricingConfigRepository::default().with_config({
        let mut config = domain::pricing::PricingConfig::default_config();
        config.free_tier.monitors = 7;
        config.free_tier.interval_seconds = 420;
        config.free_tier.retention_days = 4;
        config
    });
    let tenant_id = TenantId::new();

    let plan = ResolveEntitlementsUseCase::new(subscription_repository)
        .with_pricing_config_repository(std::sync::Arc::new(pricing_repository))
        .execute(ResolveEntitlements { tenant_id })
        .await
        .unwrap();

    assert_eq!(plan.max_monitors, 7);
    assert_eq!(plan.min_check_interval_seconds, 420);
    assert_eq!(plan.history_retention_days, 4);
}
PINGVERTEX_EOF

if [[ -f "crates/infra/tests/payg_charge_for_usage.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/infra/tests/payg_charge_for_usage.rs")"
  cp "crates/infra/tests/payg_charge_for_usage.rs" "$BACKUP_DIR/crates/infra/tests/payg_charge_for_usage.rs"
fi

mkdir -p "$(dirname "crates/infra/tests/payg_charge_for_usage.rs")"
cat > "crates/infra/tests/payg_charge_for_usage.rs" <<'PINGVERTEX_EOF'
use application::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use application::{ChargeForUsageError, ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::{Duration, Utc};
use domain::PAYGPricing;
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

#[allow(clippy::type_complexity)]
fn setup(
    unit_price: i64,
    version: i64,
) -> (
    Arc<dyn WalletRepository>,
    Arc<dyn PAYGPricingRepository>,
    Arc<dyn UsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());

    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version,
        operation: "proxy_check".into(),
        unit_name: "check".into(),
        unit_price,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::seconds(1),
        effective_until: None,
        active: true,
    }]));

    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let tenant = Uuid::new_v4();

    (wallet, pricing, usage, tenant)
}

#[tokio::test]
async fn duplicate_request_returns_previous_result_without_running_operation_twice() {
    let (wallet, pricing, usage, tenant) = setup(10, 7);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-1",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let calls = Arc::new(AtomicUsize::new(0));

    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 2,
        request_id: "req-1".into(),
    };

    let c = calls.clone();
    let first = uc
        .execute(input.clone(), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(())
            }
        })
        .await
        .unwrap();

    let c = calls.clone();
    let second = uc
        .execute(input, move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(())
            }
        })
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.usage_event, second.usage_event);
    assert_eq!(second.usage_event.pricing_version, 7);
    assert_eq!(second.usage_event.charge_units, 20);
}

#[tokio::test]
async fn concurrent_different_requests_never_overspend() {
    let (wallet, pricing, usage, tenant) = setup(60, 3);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-2",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(wallet.clone(), pricing, usage));
    let mut tasks = Vec::new();

    for i in 0..10 {
        let uc = uc.clone();

        tasks.push(tokio::spawn(async move {
            let input = ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: format!("concurrent-{i}"),
            };

            uc.execute(input, || async { Ok::<_, &'static str>(()) })
                .await
        }));
    }

    let mut success = 0;

    for task in tasks {
        if task.await.unwrap().is_ok() {
            success += 1;
        }
    }

    assert_eq!(success, 1);

    let final_wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(final_wallet.balance_units, 40);
}

#[tokio::test]
async fn insufficient_balance_is_rejected_and_wallet_never_goes_negative() {
    let (wallet, pricing, usage, tenant) = setup(60, 2);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            50,
            domain::WalletTransactionType::TopUp,
            "topup-3",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);

    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "insufficient".into(),
            },
            || async { Ok::<_, &'static str>(()) },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::InsufficientBalance)
    ));

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        50
    );
}

#[tokio::test]
async fn failed_operation_releases_reservation_and_does_not_debit() {
    let (wallet, pricing, usage, tenant) = setup(25, 9);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-4",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 2,
                request_id: "failed-op".into(),
            },
            || async { Err::<(), _>("downstream failed") },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationFailed(_))
    ));

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        100
    );

    let event = usage
        .find_by_request(tenant, "failed-op")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(event.status, "FAILED");
    assert_eq!(event.units_billed, 0);
}

#[tokio::test]
async fn expired_pending_reservation_can_be_reused_after_worker_recovery() {
    let (wallet, pricing, usage, tenant) = setup(10, 11);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-expired",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 1,
        request_id: "recoverable-request".into(),
    };

    // Simulate a worker crash after reservation creation by directly reserving
    // an already-expired reservation with the same idempotency key.
    wallet
        .reserve(
            tenant,
            10,
            "proxy_check",
            &input.request_id,
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    let result = uc
        .execute(input, || async { Ok::<_, &'static str>(()) })
        .await
        .unwrap();

    assert_eq!(result.usage_event.status, "CAPTURED");
    assert_eq!(result.usage_event.charge_units, 10);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        90
    );
}

#[tokio::test]
async fn monitoring_attempt_is_billed_when_operation_fails() {
    let (wallet, pricing, usage, tenant) = setup(25, 12);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "attempt-topup",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let result = uc
        .execute_attempt(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "attempt-failure".into(),
            },
            || async { Err::<(), _>("HTTP 500") },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationFailed(_))
    ));
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        75
    );
    let event = usage
        .find_by_request(tenant, "attempt-failure")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.status, "CAPTURED");
    assert_eq!(event.charge_units, 25);
}

#[tokio::test]
async fn expired_reservation_is_terminally_marked_then_reopened_safely() {
    let (wallet, pricing, usage, tenant) = setup(10, 13);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "expiry-topup",
            None,
        )
        .await
        .unwrap();
    wallet
        .reserve(
            tenant,
            10,
            "proxy_check",
            "expiry-retry",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    assert_eq!(wallet.expire_pending_reservations(50).await.unwrap(), 1);
    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);
    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "expiry-retry".into(),
            },
            || async { Ok::<_, &'static str>(()) },
        )
        .await
        .unwrap();

    assert_eq!(result.usage_event.charge_units, 10);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        90
    );
}

#[tokio::test]
async fn classified_pre_attempt_failure_releases_reservation_without_debit() {
    let (wallet, pricing, usage, tenant) = setup(25, 20);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "classified-preflight-topup",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let result = uc
        .execute_classified_attempt(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "classified-preflight".into(),
            },
            || async {
                Ok::<_, &'static str>(application::MeteredAttempt {
                    attempted: false,
                    error: Some("SSRF validation blocked the target".into()),
                })
            },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationNotAttempted(_))
    ));
    assert_eq!(
        wallet.get_or_create(tenant, "USD").await.unwrap().balance_units,
        100
    );
    assert!(usage
        .find_by_request(tenant, "classified-preflight")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn classified_attempt_failure_is_captured_and_billed_once() {
    let (wallet, pricing, usage, tenant) = setup(25, 21);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "classified-attempt-topup",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 1,
        request_id: "classified-attempt".into(),
    };
    let result = uc
        .execute_classified_attempt(input.clone(), || async {
            Ok::<_, &'static str>(application::MeteredAttempt {
                attempted: true,
                error: Some("connection refused".into()),
            })
        })
        .await;

    assert!(matches!(result, Err(ChargeForUsageError::OperationFailed(_))));
    assert_eq!(wallet.get_or_create(tenant, "USD").await.unwrap().balance_units, 75);

    let replay = uc
        .execute_classified_attempt(input, || async {
            panic!("idempotent replay must not execute the operation");
        })
        .await
        .unwrap();
    assert_eq!(replay.0.usage_event.charge_units, 25);
    assert_eq!(wallet.get_or_create(tenant, "USD").await.unwrap().balance_units, 75);
}

#[tokio::test]
async fn crash_after_attempt_before_capture_is_recoverable_without_double_debit() {
    let (wallet, pricing, usage, tenant) = setup(10, 22);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "crash-after-attempt-topup",
            None,
        )
        .await
        .unwrap();

    wallet
        .reserve(
            tenant,
            10,
            "proxy_check",
            "crash-after-attempt",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();
    // The external attempt is assumed to have happened immediately before a
    // worker crash. The reservation expiry worker makes the durable request
    // reusable; the retry performs the logical execution once more but still
    // permits only one ledger debit for the request.
    wallet.expire_pending_reservations(50).await.unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let result = uc
        .execute_classified_attempt(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "crash-after-attempt".into(),
            },
            || async {
                Ok::<_, &'static str>(application::MeteredAttempt {
                    attempted: true,
                    error: None,
                })
            },
        )
        .await
        .unwrap();

    assert_eq!(result.0.usage_event.charge_units, 10);
    assert_eq!(wallet.get_or_create(tenant, "USD").await.unwrap().balance_units, 90);
}

#[tokio::test]
async fn concurrent_same_execution_id_can_debit_at_most_once() {
    let (wallet, pricing, usage, tenant) = setup(10, 23);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "same-execution-topup",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone()));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let uc = uc.clone();
        tasks.push(tokio::spawn(async move {
            uc.execute_classified_attempt(
                ChargeForUsageInput {
                    tenant_id: tenant,
                    operation: "proxy_check".into(),
                    units: 1,
                    request_id: "same-execution".into(),
                },
                || async {
                    Ok::<_, &'static str>(application::MeteredAttempt {
                        attempted: true,
                        error: None,
                    })
                },
            )
            .await
        }));
    }
    for task in tasks {
        let _ = task.await.unwrap();
    }

    assert_eq!(wallet.get_or_create(tenant, "USD").await.unwrap().balance_units, 90);
    let event = usage.find_by_request(tenant, "same-execution").await.unwrap();
    assert!(event.is_some());
}
PINGVERTEX_EOF

if [[ -f "crates/infra/tests/scheduler_claim.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/infra/tests/scheduler_claim.rs")"
  cp "crates/infra/tests/scheduler_claim.rs" "$BACKUP_DIR/crates/infra/tests/scheduler_claim.rs"
fi

mkdir -p "$(dirname "crates/infra/tests/scheduler_claim.rs")"
cat > "crates/infra/tests/scheduler_claim.rs" <<'PINGVERTEX_EOF'
use std::time::Duration;

use application::ports::SchedulingRepository;
use domain::shared::ids::MonitorId;
use infra::PostgresSchedulingRepository;
use serial_test::serial;
use sqlx::PgPool;
use uuid::Uuid;

mod common;

async fn insert_due_monitor(pool: &PgPool, name: &str, user_id: Uuid, tenant_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO monitors (
            id, user_id, tenant_id, regions, name, url, method,
            expected_status, interval_seconds, timeout_seconds,
            enabled, ssl_check_enabled, ssl_expiry_warning_days, kind,
            created_at, updated_at, next_due_at
        )
        VALUES (
            $1, $2, $3, ARRAY['us-east'], $4, 'http://example.com', 'GET',
            200, 60, 10,
            true, true, 14, 'http',
            now(), now(), now() - interval '1 minute'
        )
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(tenant_id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[tokio::test]
#[serial]
async fn concurrent_claim_attempts_do_not_double_claim() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;

    let mut ids = Vec::new();
    for i in 0..20 {
        ids.push(insert_due_monitor(&pool, &format!("monitor-{i}"), user_id, tenant_id.0).await);
    }

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let lease_duration = Duration::from_secs(60);
    let now = chrono::Utc::now();

    let (res1, res2) = tokio::join!(
        repo.claim_due_monitors(10, Uuid::new_v4(), lease_duration, now),
        repo.claim_due_monitors(10, Uuid::new_v4(), lease_duration, now)
    );

    let mut claimed_ids = Vec::new();
    for monitor in res1.unwrap().into_iter().chain(res2.unwrap()) {
        claimed_ids.push(monitor.id.0);
    }

    // Concurrent callers must never double-claim a monitor.
    //
    // The two calls may race such that one caller returns fewer rows,
    // but the database state must still contain all successfully claimed
    // monitors and no monitor may be returned twice.
    assert!(
        claimed_ids.len() <= 20,
        "concurrent claimers must not claim more than the available monitors"
    );

    let claimed_count = claimed_ids.len();

    claimed_ids.sort();
    claimed_ids.dedup();

    assert_eq!(
        claimed_ids.len(),
        claimed_count,
        "concurrent claimers must never return the same monitor twice"
    );

    let leased_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM monitors WHERE lease_owner IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        leased_count, 20,
        "all 20 due monitors must be leased after the concurrent claims"
    );
}

#[tokio::test]
#[serial]
async fn expired_lease_makes_monitor_claimable_again() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "crash-recovery", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let lease_duration = Duration::from_secs(60);

    // First claim succeeds.
    let first_owner = Uuid::new_v4();
    let claimed = repo
        .claim_due_monitors(1, first_owner, lease_duration, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id.0, monitor_id);
    let first_claimed_at = claimed[0].claimed_at;
    let first_execution_id = claimed[0].execution_id;

    // Simulate crash: manually force lease expiry.
    sqlx::query("UPDATE monitors SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(monitor_id)
        .execute(&pool)
        .await
        .unwrap();

    // Recovery clears expired lease.
    let recovered = repo
        .recover_expired_leases(chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(recovered, 1);

    // Second claim succeeds after recovery.
    let second_owner = Uuid::new_v4();
    let claimed_again = repo
        .claim_due_monitors(1, second_owner, lease_duration, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(claimed_again.len(), 1);
    assert_eq!(claimed_again[0].id.0, monitor_id);
    assert_eq!(claimed_again[0].claimed_at, first_claimed_at);
    assert_eq!(claimed_again[0].execution_id, first_execution_id);

    // Complete claim clears lease and advances next_due_at.
    repo.complete_claim(MonitorId(monitor_id), second_owner)
        .await
        .unwrap();
    let lease_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(lease_owner.is_none());

    let next_due_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(next_due_at > chrono::Utc::now());
}

#[tokio::test]
#[serial]
async fn immediate_check_request_is_tenant_scoped_and_preserves_leases() {
    let pool = common::setup_pool().await;
    let user_a = Uuid::new_v4();
    let tenant_a = common::insert_user(&pool, domain::UserId(user_a)).await;
    let user_b = Uuid::new_v4();
    let tenant_b = common::insert_user(&pool, domain::UserId(user_b)).await;

    let a1 = insert_due_monitor(&pool, "manual-a-1", user_a, tenant_a.0).await;
    let a2 = insert_due_monitor(&pool, "manual-a-2", user_a, tenant_a.0).await;
    let b1 = insert_due_monitor(&pool, "manual-b-1", user_b, tenant_b.0).await;

    // Keep tenant B's monitor in the future so the test can verify
    // that request_immediate_checks() does not affect another tenant.
    sqlx::query("UPDATE monitors SET next_due_at = now() + interval '10 minutes' WHERE id = $1")
        .bind(b1)
        .execute(&pool)
        .await
        .unwrap();

    let lease_owner = Uuid::new_v4();
    sqlx::query("UPDATE monitors SET lease_owner = $1, lease_expires_at = now() + interval '60 seconds', lease_claimed_at = now(), next_due_at = now() + interval '10 minutes' WHERE id = $2")
        .bind(lease_owner)
        .bind(a2)
        .execute(&pool)
        .await
        .unwrap();

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let queued = repo.request_immediate_checks(tenant_a).await.unwrap();
    assert_eq!(queued, 2);

    let a1_due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(a1)
            .fetch_one(&pool)
            .await
            .unwrap();
    let a2_due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(a2)
            .fetch_one(&pool)
            .await
            .unwrap();
    let b1_due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_due_at FROM monitors WHERE id = $1")
            .bind(b1)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(a1_due <= chrono::Utc::now());
    assert!(a2_due <= chrono::Utc::now());
    assert!(b1_due > chrono::Utc::now());

    let preserved_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(a2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preserved_owner, Some(lease_owner));
}

#[tokio::test]
#[serial]
async fn stale_worker_cannot_complete_a_reclaimed_monitor_claim() {
    let pool = common::setup_pool().await;
    let user_id = Uuid::new_v4();
    let tenant_id = common::insert_user(&pool, domain::UserId(user_id)).await;
    let monitor_id = insert_due_monitor(&pool, "stale-owner", user_id, tenant_id.0).await;

    let repo = PostgresSchedulingRepository::new(pool.clone());
    let first_owner = Uuid::new_v4();
    let second_owner = Uuid::new_v4();
    let first = repo
        .claim_due_monitors(1, first_owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(first.len(), 1);

    sqlx::query("UPDATE monitors SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(monitor_id)
        .execute(&pool)
        .await
        .unwrap();
    repo.recover_expired_leases(chrono::Utc::now())
        .await
        .unwrap();

    let second = repo
        .claim_due_monitors(1, second_owner, Duration::from_secs(60), chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(second.len(), 1);

    let stale_result = repo
        .complete_claim(MonitorId(monitor_id), first_owner)
        .await;
    assert!(stale_result.is_err());

    let current_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT lease_owner FROM monitors WHERE id = $1")
            .bind(monitor_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current_owner, Some(second_owner));

    repo.complete_claim(MonitorId(monitor_id), second_owner)
        .await
        .unwrap();
}
PINGVERTEX_EOF

if [[ -f "crates/worker/tests/retention_worker.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/tests/retention_worker.rs")"
  cp "crates/worker/tests/retention_worker.rs" "$BACKUP_DIR/crates/worker/tests/retention_worker.rs"
fi

mkdir -p "$(dirname "crates/worker/tests/retention_worker.rs")"
cat > "crates/worker/tests/retention_worker.rs" <<'PINGVERTEX_EOF'
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use application::ports::{HistoryRetentionRepository, SubscriptionRepository, TenantRepository};
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use domain::{
    Check, CheckStatus, ErrorType, PlanTier, RegionCode, Subscription, Tenant,
    shared::ids::{CheckId, MonitorId, SubscriptionId, TenantId, UserId},
};
use infra::{
    InMemoryHistoryRetentionRepository, InMemoryPricingConfigRepository, InMemorySubscriptionRepository, InMemoryTenantRepository,
};
use uuid::Uuid;
use worker::{RetentionWorker, retention_worker::DEFAULT_BATCH_SIZE};

fn sample_check_older_than_days(days: i64) -> Check {
    let mut check = Check::new(
        CheckId(Uuid::new_v4()),
        MonitorId(Uuid::new_v4()),
        RegionCode::default_region(),
        CheckStatus::Up,
        Some(200),
        123,
        ErrorType::None,
        None,
    );
    check.checked_at = Utc::now() - ChronoDuration::days(days);
    check
}

fn tenant(name: &str) -> Tenant {
    Tenant::new(TenantId::new(), name, UserId::new()).unwrap()
}

#[tokio::test]
async fn retention_deletes_older_than_cutoff_per_plan() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let free_tenant = tenant("Free Tenant");
    let starter_tenant = tenant("Starter Tenant");
    tenant_repo.create(&free_tenant).await.unwrap();
    tenant_repo.create(&starter_tenant).await.unwrap();

    subscription_repo
        .create(&Subscription::new(
            SubscriptionId::new(),
            free_tenant.id,
            PlanTier::Free,
        ))
        .await
        .unwrap();
    subscription_repo
        .create(&Subscription::new(
            SubscriptionId::new(),
            starter_tenant.id,
            PlanTier::Starter,
        ))
        .await
        .unwrap();

    // Free retains 2 days; Starter retains 30 days.
    let old_free = sample_check_older_than_days(3);
    let recent_free = sample_check_older_than_days(0);
    let old_starter = sample_check_older_than_days(10);

    history_repo.seed_check(old_free.clone(), free_tenant.id);
    history_repo.seed_check(recent_free.clone(), free_tenant.id);
    history_repo.seed_check(old_starter.clone(), starter_tenant.id);

    let worker = RetentionWorker::new(
        tenant_repo,
        subscription_repo,
        history_repo.clone(),
        Arc::new(InMemoryPricingConfigRepository::default()),
        100,
        Duration::from_secs(3600),
        2,
    );

    let summary = worker.run_sweep().await.unwrap();

    assert_eq!(summary.tenants_processed, 2);
    assert_eq!(summary.checks_deleted, 1); // Only old_free is beyond its plan retention.

    let remaining_ids = history_repo.list_check_ids();
    assert!(remaining_ids.contains(&recent_free.id));
    assert!(remaining_ids.contains(&old_starter.id));
    assert!(!remaining_ids.contains(&old_free.id));
}

#[tokio::test]
async fn batching_does_not_delete_more_or_less_than_expected() {
    let tenant_id = TenantId::new();
    let repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    for _ in 0..5 {
        repo.seed_check(sample_check_older_than_days(2), tenant_id);
    }

    let cutoff = Utc::now() - ChronoDuration::days(2);
    let batch_size = 2;

    let deleted1 = repo
        .delete_older_than_batch(tenant_id, cutoff, batch_size)
        .await
        .unwrap();
    let deleted2 = repo
        .delete_older_than_batch(tenant_id, cutoff, batch_size)
        .await
        .unwrap();
    let deleted3 = repo
        .delete_older_than_batch(tenant_id, cutoff, batch_size)
        .await
        .unwrap();

    assert_eq!(deleted1, 2);
    assert_eq!(deleted2, 2);
    assert_eq!(deleted3, 1);
    assert!(repo.list_check_ids().is_empty());
}

#[tokio::test]
async fn running_twice_is_idempotent() {
    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    let tenant = tenant("Idempotent Tenant");
    tenant_repo.create(&tenant).await.unwrap();
    subscription_repo
        .create(&Subscription::new(
            SubscriptionId::new(),
            tenant.id,
            PlanTier::Free,
        ))
        .await
        .unwrap();

    // Free retention is 2 days. Keep one fresh check and two checks beyond the cutoff.
    let old1 = sample_check_older_than_days(3);
    let old2 = sample_check_older_than_days(4);
    let recent = sample_check_older_than_days(0);

    history_repo.seed_check(old1.clone(), tenant.id);
    history_repo.seed_check(old2.clone(), tenant.id);
    history_repo.seed_check(recent.clone(), tenant.id);

    let worker = RetentionWorker::new(
        tenant_repo,
        subscription_repo,
        history_repo.clone(),
        Arc::new(InMemoryPricingConfigRepository::default()),
        DEFAULT_BATCH_SIZE,
        Duration::from_secs(3600),
        2,
    );

    let first = worker.run_sweep().await.unwrap();
    assert_eq!(first.checks_deleted, 2);

    let second = worker.run_sweep().await.unwrap();
    assert_eq!(second.checks_deleted, 0);

    let remaining = history_repo.list_check_ids();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0], recent.id);
}

/// H16 Task A regression test: wraps `HistoryRetentionRepository` and
/// records how many `delete_older_than_batch` calls are in flight at
/// once. Proves `RetentionWorker::run_sweep`'s semaphore actually caps
/// concurrent per-tenant work at `max_concurrent_tenants` — not just
/// that the code compiles with a `Semaphore` in it — and that every
/// tenant still gets processed despite the bound (no starvation /
/// resource exhaustion).
struct ConcurrencyTrackingHistoryRepository<H> {
    inner: Arc<H>,
    in_flight: Arc<AtomicUsize>,
    max_observed: Arc<AtomicUsize>,
    delay: Duration,
}

#[async_trait]
impl<H> HistoryRetentionRepository for ConcurrencyTrackingHistoryRepository<H>
where
    H: HistoryRetentionRepository + Send + Sync,
{
    async fn delete_older_than_batch(
        &self,
        tenant_id: TenantId,
        cutoff: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_observed.fetch_max(current, Ordering::SeqCst);

        tokio::time::sleep(self.delay).await;

        let result = self
            .inner
            .delete_older_than_batch(tenant_id, cutoff, batch_size)
            .await;

        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

#[tokio::test]
async fn bounded_concurrency_never_exceeds_configured_limit() {
    const TENANT_COUNT: usize = 12;
    const MAX_CONCURRENT_TENANTS: usize = 3;

    let tenant_repo = Arc::new(InMemoryTenantRepository::new());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
    let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());

    for i in 0..TENANT_COUNT {
        let t = tenant(&format!("Concurrency Tenant {i}"));
        tenant_repo.create(&t).await.unwrap();
        subscription_repo
            .create(&Subscription::new(
                SubscriptionId::new(),
                t.id,
                PlanTier::Free,
            ))
            .await
            .unwrap();
        history_repo.seed_check(sample_check_older_than_days(10), t.id);
    }

    let tracking_repo = Arc::new(ConcurrencyTrackingHistoryRepository {
        inner: history_repo,
        in_flight: Arc::new(AtomicUsize::new(0)),
        max_observed: Arc::new(AtomicUsize::new(0)),
        delay: Duration::from_millis(50),
    });

    let worker = RetentionWorker::new(
        tenant_repo,
        subscription_repo,
        tracking_repo.clone(),
        Arc::new(InMemoryPricingConfigRepository::default()),
        100,
        Duration::from_secs(3600),
        MAX_CONCURRENT_TENANTS,
    );

    let summary = worker.run_sweep().await.unwrap();

    assert_eq!(summary.tenants_processed, TENANT_COUNT as u64);
    assert_eq!(summary.checks_deleted, TENANT_COUNT as u64);

    let max_observed = tracking_repo.max_observed.load(Ordering::SeqCst);
    assert!(
        max_observed <= MAX_CONCURRENT_TENANTS,
        "observed {max_observed} tenants being processed concurrently, expected at most \
         {MAX_CONCURRENT_TENANTS} — bounded concurrency is not being respected \
         (resource exhaustion risk)"
    );
    assert!(
        max_observed >= 2,
        "expected genuine concurrent processing (at least 2 tenants in flight at once), \
         only observed {max_observed} — sweep may have regressed to fully sequential"
    );
}
PINGVERTEX_EOF

if [[ -f "crates/worker/tests/worker_graceful_shutdown.rs" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "crates/worker/tests/worker_graceful_shutdown.rs")"
  cp "crates/worker/tests/worker_graceful_shutdown.rs" "$BACKUP_DIR/crates/worker/tests/worker_graceful_shutdown.rs"
fi

mkdir -p "$(dirname "crates/worker/tests/worker_graceful_shutdown.rs")"
cat > "crates/worker/tests/worker_graceful_shutdown.rs" <<'PINGVERTEX_EOF'
//! H16 Task B regression tests: every background worker that accepts a
//! `CancellationToken` must actually stop its `run_forever` loop once
//! that token is cancelled, within a bounded time — not hang forever
//! waiting on the next tick. Each test cancels the token BEFORE the
//! worker starts, then asserts the spawned task completes within a
//! generous timeout, proving the cancellation branch of
//! `tokio::select!` is reachable and correctly shuts the loop down.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use application::ports::{CustomDomainVerifier, IncidentRepository, NotificationService};
use domain::{
    Incident,
    entities::NotificationChannel,
    notification::NotificationEvent,
    shared::ids::{IncidentId, MonitorId},
};
use infra::{
    InMemoryEscalationPolicyRepository, InMemoryHistoryRetentionRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryNotificationOutboxRepository, InMemoryProxyCheckJobRepository,
    InMemoryPricingConfigRepository,
    InMemoryStatusPageRepository, InMemorySubscriptionRepository, InMemoryTenantRepository,
};
use worker::{
    CustomDomainVerificationWorker, EscalationWorker, HeartbeatSweeper, OutboxWorker,
    ProxyCheckWorker, RetentionWorker,
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Minimal `IncidentRepository` double. Only `list_open_unacknowledged`
/// (defaulted to `Ok(vec![])`) can actually be reached by
/// `EscalationWorker::run_once` in these tests; the rest are
/// unreachable stubs.
struct NoOpIncidentRepository;

#[async_trait]
impl IncidentRepository for NoOpIncidentRepository {
    async fn save(
        &self,
        _incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn update(
        &self,
        _incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn find_open_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

/// No-op notification sender — never actually reached, since
/// `NoOpIncidentRepository` never returns any incidents to escalate.
struct NoOpNotificationService;

#[async_trait]
impl NotificationService for NoOpNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// No-op DNS verifier — never actually reached, since
/// `InMemoryStatusPageRepository::new()` starts with zero pending
/// domains.
struct NoOpCustomDomainVerifier;

#[async_trait]
impl CustomDomainVerifier for NoOpCustomDomainVerifier {
    async fn verify(
        &self,
        _domain: &str,
        _expected_token: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
}

#[tokio::test]
async fn escalation_worker_stops_gracefully_on_cancellation() {
    let worker = EscalationWorker::new(
        Arc::new(NoOpIncidentRepository),
        Arc::new(InMemoryMonitorRepository::new()),
        Arc::new(InMemoryEscalationPolicyRepository::new()),
        Arc::new(InMemoryNotificationChannelRepository::new(
            InMemoryMonitorRepository::new(),
        )),
        Arc::new(NoOpNotificationService),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("escalation worker did not shut down within timeout")
        .expect("escalation worker task panicked");

    assert!(
        outcome.is_ok(),
        "escalation worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn custom_domain_verification_worker_stops_gracefully_on_cancellation() {
    let worker = CustomDomainVerificationWorker::new(
        Arc::new(InMemoryStatusPageRepository::new()),
        Arc::new(NoOpCustomDomainVerifier),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("custom domain verification worker did not shut down within timeout")
        .expect("custom domain verification worker task panicked");

    assert!(
        outcome.is_ok(),
        "custom domain verification worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn proxy_check_worker_stops_gracefully_on_cancellation() {
    let wallet = Arc::new(infra::InMemoryWalletRepository::default());
    let pricing = Arc::new(infra::InMemoryPAYGPricingRepository::default());
    let usage = Arc::new(infra::InMemoryUsageEventRepository::default());
    let metering = Arc::new(application::MeterUsageUseCase::new(wallet, pricing, usage));
    let worker = ProxyCheckWorker::new(Arc::new(InMemoryProxyCheckJobRepository::new()), metering);

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("proxy check worker did not shut down within timeout")
        .expect("proxy check worker task panicked");

    assert!(
        outcome.is_ok(),
        "proxy check worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn heartbeat_sweeper_stops_gracefully_on_cancellation() {
    let sweeper = HeartbeatSweeper::new(Arc::new(InMemoryMonitorRepository::new()));

    let token = CancellationToken::new();
    token.cancel();

    let handle =
        tokio::spawn(async move { sweeper.run_forever(Duration::from_millis(10), token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("heartbeat sweeper did not shut down within timeout")
        .expect("heartbeat sweeper task panicked");

    assert!(
        outcome.is_ok(),
        "heartbeat sweeper returned an error on shutdown"
    );
}

#[tokio::test]
async fn outbox_worker_stops_gracefully_on_cancellation() {
    let worker = OutboxWorker::new(
        Arc::new(InMemoryNotificationOutboxRepository::new()),
        Arc::new(worker::outbox_worker::StubNotificationOutboxSender::new()),
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("outbox worker did not shut down within timeout")
        .expect("outbox worker task panicked");

    assert!(
        outcome.is_ok(),
        "outbox worker returned an error on shutdown"
    );
}

#[tokio::test]
async fn retention_worker_stops_gracefully_on_cancellation() {
    let worker = RetentionWorker::new(
        Arc::new(InMemoryTenantRepository::new()),
        Arc::new(InMemorySubscriptionRepository::new()),
        Arc::new(InMemoryHistoryRetentionRepository::new()),
        Arc::new(InMemoryPricingConfigRepository::default()),
        worker::retention_worker::DEFAULT_BATCH_SIZE,
        Duration::from_millis(10),
        2,
    );

    let token = CancellationToken::new();
    token.cancel();

    let handle = tokio::spawn(async move { worker.run_forever(token).await });

    let outcome = tokio::time::timeout(SHUTDOWN_TIMEOUT, handle)
        .await
        .expect("retention worker did not shut down within timeout")
        .expect("retention worker task panicked");

    assert!(
        outcome.is_ok(),
        "retention worker returned an error on shutdown"
    );
}
PINGVERTEX_EOF

if [[ -f "docs/PAYG_RUNTIME_CONTRACT.md" ]]; then
  mkdir -p "$BACKUP_DIR/$(dirname "docs/PAYG_RUNTIME_CONTRACT.md")"
  cp "docs/PAYG_RUNTIME_CONTRACT.md" "$BACKUP_DIR/docs/PAYG_RUNTIME_CONTRACT.md"
fi

mkdir -p "$(dirname "docs/PAYG_RUNTIME_CONTRACT.md")"
cat > "docs/PAYG_RUNTIME_CONTRACT.md" <<'PINGVERTEX_EOF'
# PAYG Runtime Contract

## Billing boundary

`monitor_check` is authorized before the real HTTP/TCP/DNS/Flow attempt. The scheduler supplies a durable execution identity derived from the database lease execution id. A debit is captured only when the executor reports that the billable external attempt actually started.

## Billable attempts

- HTTP: a request after SSRF validation starts the billable attempt. HTTP 4xx/5xx and network errors after request start are billable. SSRF rejection, disabled-monitor validation, malformed configuration, and client-construction failures before request start are not billable.
- TCP: DNS/target resolution must succeed first; `TcpStream::connect` starting is the billable attempt. Refusal and timeout are billable. Invalid/missing port and pre-connect validation failures are not billable.
- DNS: issuing the resolver lookup is the billable attempt. No-record and resolver failures are billable because the lookup was attempted; malformed record configuration is not.
- Flow: a flow is billable once at least one HTTP step request starts. Placeholder resolution, missing flow configuration, SSRF rejection, and other failures before any request starts are not billable.
- Proxy: each proxy entry has its own durable request id (`proxy-check:<job>:<entry>`) and is metered independently. A successful, refused, timed-out, protocol-failed, or authentication-failed proxy attempt is billable once the proxy connection attempt starts. Private-IP/invalid-proxy preflight failures are not billable.

## Insufficient balance

Reservation occurs before the external attempt. If the wallet cannot reserve the server-resolved price, no network operation is started and the wallet enters `PAYG_BALANCE_EXHAUSTED`. The dashboard exposes the blocked state.

## Crash/retry

Reservations are short-lived and expired by `WalletExpirationWorker`. A scheduler lease keeps the same execution id through lease recovery. Wallet reservations and ledger debits are idempotent by tenant/request id and reservation id. Captures write the debit, usage event, and financial audit records in the same database transaction.

## Ledger/source of truth

`wallet_transactions` is append-only. `api_usage_events` is immutable and uniquely keyed by `(tenant_id, request_id)`. PAYG prices are immutable and server-resolved. The Free tier's monitor count, minimum interval, and retention values are resolved from the active database `pricing_configs.free_*` fields; the static `Plan` catalog supplies paid-tier capabilities.

## Reconciliation

The reconciliation worker expires stale reservations, repairs legacy captured-debit usage gaps where historical pricing is available, repairs missing financial audit rows, and reports duplicate/mismatch invariants. PAYG reconciliation is independent of payment-provider webhook reconciliation.
PINGVERTEX_EOF

echo "PAYG remediation files written."
echo "Backup: $BACKUP_DIR"
echo
echo "Next: run these from the project root:"
echo "  cargo fmt --all -- --check"
echo "  cargo test --workspace"
echo "  cargo test -p infra --test payg_charge_for_usage"
echo "  cargo test -p infra --test scheduler_claim"
echo "  cargo test -p application --test resolve_entitlements_use_case"
echo "  cargo test -p worker --test retention_worker"
echo "  cargo test -p worker --test worker_graceful_shutdown"
echo "  ./scripts/test_database_migrations.sh"
