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

    pub async fn find_usage_event(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> Result<Option<UsageEvent>, ChargeForUsageError> {
        self.usage_event_repository
            .find_by_request(tenant_id, request_id)
            .await
            .map_err(ChargeForUsageError::Repository)
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
    /// Finalizes an execution that was reserved before it ran (used by
    /// remote agents). The operation has already happened, so this method
    /// never invokes external I/O; it only captures/releases the durable
    /// reservation and records the immutable usage event.
    pub async fn finalize_reserved_attempt(
        &self,
        input: ChargeForUsageInput,
        attempted: bool,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError> {
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
                pricing.unit_price,
                pricing.version,
                &pricing.currency,
                &input.request_id,
                Utc::now() + Duration::seconds(120),
            )
            .await
            .map_err(|e| {
                if e.to_string() == "insufficient wallet balance" {
                    ChargeForUsageError::InsufficientBalance
                } else {
                    ChargeForUsageError::Repository(e)
                }
            })?;

        if !attempted {
            self.wallet_repository
                .release(reservation.reservation.id)
                .await
                .map_err(ChargeForUsageError::Repository)?;
            return Err(ChargeForUsageError::OperationNotAttempted(
                "pre-attempt failure".to_owned(),
            ));
        }

        let reserved_unit_price = reservation
            .reservation
            .unit_price
            .unwrap_or(pricing.unit_price);
        let reserved_pricing_version = reservation
            .reservation
            .pricing_version
            .unwrap_or(pricing.version);
        let reserved_currency = reservation
            .reservation
            .currency
            .as_deref()
            .unwrap_or(&pricing.currency);
        let event = self.build_event(
            &input,
            input.units,
            reserved_unit_price,
            reservation.reservation.amount_units,
            reserved_pricing_version,
            reserved_currency,
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

        Ok(ChargeForUsageResult {
            usage_event: event,
            transaction: Some(transaction),
        })
    }

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
                pricing.unit_price,
                pricing.version,
                &pricing.currency,
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
        let reserved_unit_price = reservation
            .reservation
            .unit_price
            .unwrap_or(pricing.unit_price);
        let reserved_pricing_version = reservation
            .reservation
            .pricing_version
            .unwrap_or(pricing.version);
        let reserved_currency = reservation
            .reservation
            .currency
            .as_deref()
            .unwrap_or(&pricing.currency);
        let event = self.build_event(
            &input,
            input.units,
            reserved_unit_price,
            reservation.reservation.amount_units,
            reserved_pricing_version,
            reserved_currency,
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
        &self,
        input: ChargeForUsageInput,
        operation: F,
    ) -> Result<(ChargeForUsageResult, MeteredAttempt), ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<MeteredAttempt, E>>,
        E: fmt::Display,
    {
        self.execute_classified_attempt_with_output(input, || async {
            operation().await.map(|attempt| (attempt.clone(), attempt))
        })
        .await
        .map(|(result, attempt)| {
            (
                result,
                attempt.unwrap_or(MeteredAttempt {
                    // A replay means the durable execution was already
                    // finalized by an earlier worker. The caller must not
                    // execute the external operation again.
                    attempted: true,
                    error: None,
                }),
            )
        })
    }

    pub async fn execute_classified_attempt_with_output<F, Fut, E, T>(
        &self,
        input: ChargeForUsageInput,
        operation: F,
    ) -> Result<(ChargeForUsageResult, Option<T>), ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(MeteredAttempt, T), E>>,
        E: fmt::Display,
    {
        validate_input(&input)?;
        if let Some(event) = self
            .usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id)
            .await
            .map_err(ChargeForUsageError::Repository)?
        {
            return Ok((
                ChargeForUsageResult {
                    usage_event: event,
                    transaction: None,
                },
                None,
            ));
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
                pricing.unit_price,
                pricing.version,
                &pricing.currency,
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
        if !reservation.created {
            if reservation.reservation.status == CreditReservation::CAPTURED
                && let Some(event) = self
                    .usage_event_repository
                    .find_by_request(input.tenant_id, &input.request_id)
                    .await
                    .map_err(ChargeForUsageError::Repository)?
            {
                return Ok((
                    ChargeForUsageResult {
                        usage_event: event,
                        transaction: None,
                    },
                    None,
                ));
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
                self.wallet_repository
                    .release(reservation.reservation.id)
                    .await
                    .map_err(ChargeForUsageError::Repository)?;
                return Err(ChargeForUsageError::OperationNotAttempted(err.to_string()));
            }
        };
        if !attempt.attempted {
            self.wallet_repository
                .release(reservation.reservation.id)
                .await
                .map_err(ChargeForUsageError::Repository)?;
            return Err(ChargeForUsageError::OperationNotAttempted(
                attempt
                    .error
                    .unwrap_or_else(|| "pre-attempt failure".to_owned()),
            ));
        }
        let reserved_unit_price = reservation
            .reservation
            .unit_price
            .unwrap_or(pricing.unit_price);
        let reserved_pricing_version = reservation
            .reservation
            .pricing_version
            .unwrap_or(pricing.version);
        let reserved_currency = reservation
            .reservation
            .currency
            .as_deref()
            .unwrap_or(&pricing.currency);
        let event = self.build_event(
            &input,
            input.units,
            reserved_unit_price,
            reservation.reservation.amount_units,
            reserved_pricing_version,
            reserved_currency,
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
        if let Some(err) = attempt.error {
            return Err(ChargeForUsageError::OperationFailed(format!(
                "{err}; usage_event={}",
                event.id
            )));
        }
        Ok((
            ChargeForUsageResult {
                usage_event: event,
                transaction: Some(transaction),
            },
            Some(output),
        ))
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
                pricing.unit_price,
                pricing.version,
                &pricing.currency,
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

        let reserved_unit_price = reservation.unit_price.unwrap_or(pricing.unit_price);
        let reserved_pricing_version = reservation.pricing_version.unwrap_or(pricing.version);
        let reserved_currency = reservation.currency.as_deref().unwrap_or(&pricing.currency);
        let event = self.build_event(
            &input,
            input.units,
            reserved_unit_price,
            reservation.amount_units,
            reserved_pricing_version,
            reserved_currency,
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
    if !domain::PaygOperation::is_canonical(input.operation.trim()) {
        return Err(ChargeForUsageError::InvalidInput(
            "unsupported PAYG operation",
        ));
    }
    if input.request_id.trim().is_empty() {
        return Err(ChargeForUsageError::InvalidInput("request_id is required"));
    }
    Ok(())
}
