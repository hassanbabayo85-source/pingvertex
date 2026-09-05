use crate::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
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

#[derive(Debug)]
pub enum ChargeForUsageError {
    InvalidInput(&'static str),
    NoPricing,
    InvalidPricing(&'static str),
    InsufficientBalance,
    OperationFailed(String),
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
        }
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
            // operation. Never execute the billable operation twice.
            if reservation_outcome.reservation.status == CreditReservation::CAPTURED {
                let event = self
                    .recover_captured_event(
                        &input,
                        &pricing.version,
                        charge_units,
                        &pricing.currency,
                        &pricing.unit_price,
                    )
                    .await?;
                return Ok(ChargeForUsageResult {
                    usage_event: event,
                    transaction: None,
                });
            }
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

        let transaction = match self.wallet_repository.capture(reservation.id).await {
            Ok(tx) => tx,
            Err(err) => {
                return Err(ChargeForUsageError::CaptureFailed(err));
            }
        };

        let event = self
            .record_event(
                &input,
                input.units,
                pricing.unit_price,
                charge_units,
                pricing.version,
                &pricing.currency,
                "CAPTURED",
            )
            .await?;

        Ok(ChargeForUsageResult {
            usage_event: event,
            transaction: Some(transaction),
        })
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
        self.usage_event_repository
            .create(&UsageEvent {
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
            })
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
    ) -> Result<UsageEvent, ChargeForUsageError> {
        if let Some(event) = self
            .usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id)
            .await
            .map_err(ChargeForUsageError::Repository)?
        {
            return Ok(event);
        }
        self.record_event(
            input,
            input.units,
            *unit_price,
            charge_units,
            *pricing_version,
            currency,
            "CAPTURED",
        )
        .await
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
