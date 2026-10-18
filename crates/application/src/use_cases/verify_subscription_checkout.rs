//! Task 6 — verify a subscription checkout server-side.
//!
//! The activation is guarded by the `(provider, provider_reference)`
//! idempotency of `subscription_billing_records.try_insert`: only the
//! first call that succeeds with that constraint wins. Duplicate
//! verifications or webhook redeliveries are no-ops.
//!
//! Both the amount and the currency reported by the provider are
//! compared against the values captured at initiate time.

use std::error::Error;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use domain::CheckoutSession;
use domain::Subscription;
use domain::billing_period::period_end_from;
use domain::enums::{CheckoutStatus, PlanTier};
use domain::money::{BillingInterval, Currency};
use domain::shared::ids::{CheckoutSessionId, SubscriptionId, TenantId};

use crate::ports::subscription_billing_record_repository::{
    BillingRecordStatus, InsertBillingRecordOutcome, SubscriptionBillingRecord,
    SubscriptionBillingRecordRepository,
};
use crate::ports::{
    CheckoutSessionRepository, PaymentProvider, SubscriptionPaymentVerification,
    SubscriptionRepository,
};

#[derive(Debug, Clone)]
pub struct VerifySubscriptionCheckoutCommand {
    pub tenant_id: TenantId,
    pub checkout_session_id: CheckoutSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifySubscriptionCheckoutOutcome {
    pub checkout_status: CheckoutStatus,
}

#[derive(Debug)]
pub enum VerifySubscriptionCheckoutError {
    CheckoutSessionNotFound,
    CheckoutNotReady,
    Invalid(String),
    AmountMismatch {
        expected_minor: i64,
        got_minor: i64,
    },
    CurrencyMismatch {
        expected: Currency,
        got: Currency,
    },
    Subscription(domain::DomainError),
    Provider(Box<dyn Error + Send + Sync>),
    Repository(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for VerifySubscriptionCheckoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CheckoutSessionNotFound => write!(f, "checkout session not found"),
            Self::CheckoutNotReady => write!(f, "checkout session has no provider reference"),
            Self::Invalid(msg) => write!(f, "{msg}"),
            Self::AmountMismatch {
                expected_minor,
                got_minor,
            } => write!(
                f,
                "amount mismatch: expected {expected_minor} minor units, provider reported {got_minor}"
            ),
            Self::CurrencyMismatch { expected, got } => write!(
                f,
                "currency mismatch: expected {expected:?}, provider reported {got:?}"
            ),
            Self::Subscription(e) => write!(f, "subscription error: {e}"),
            Self::Provider(e) => write!(f, "provider error: {e}"),
            Self::Repository(e) => write!(f, "repository error: {e}"),
        }
    }
}

impl Error for VerifySubscriptionCheckoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Subscription(e) => Some(e),
            Self::Provider(e) | Self::Repository(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct VerifySubscriptionCheckoutUseCase {
    checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
    subscription_repository: Arc<dyn SubscriptionRepository>,
    subscription_billing_record_repository: Arc<dyn SubscriptionBillingRecordRepository>,
    payment_provider: Arc<dyn PaymentProvider>,
}

impl VerifySubscriptionCheckoutUseCase {
    pub fn new(
        checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
        subscription_repository: Arc<dyn SubscriptionRepository>,
        subscription_billing_record_repository: Arc<dyn SubscriptionBillingRecordRepository>,
        payment_provider: Arc<dyn PaymentProvider>,
    ) -> Self {
        Self {
            checkout_session_repository,
            subscription_repository,
            subscription_billing_record_repository,
            payment_provider,
        }
    }

    pub async fn execute(
        &self,
        command: VerifySubscriptionCheckoutCommand,
    ) -> Result<VerifySubscriptionCheckoutOutcome, VerifySubscriptionCheckoutError> {
        let session = self
            .checkout_session_repository
            .find_by_id(command.checkout_session_id)
            .await
            .map_err(VerifySubscriptionCheckoutError::Repository)?
            .ok_or(VerifySubscriptionCheckoutError::CheckoutSessionNotFound)?;

        if session.tenant_id != command.tenant_id {
            return Err(VerifySubscriptionCheckoutError::CheckoutSessionNotFound);
        }

        if session.status == CheckoutStatus::Completed {
            return Ok(VerifySubscriptionCheckoutOutcome {
                checkout_status: CheckoutStatus::Completed,
            });
        }

        let provider_reference = session
            .provider_checkout_id
            .clone()
            .ok_or(VerifySubscriptionCheckoutError::CheckoutNotReady)?;
        let plan_tier = session.plan_tier.ok_or_else(|| {
            VerifySubscriptionCheckoutError::Invalid(
                "subscription checkout session is missing plan_tier".to_owned(),
            )
        })?;
        let billing_interval = session.billing_interval.ok_or_else(|| {
            VerifySubscriptionCheckoutError::Invalid(
                "subscription checkout session is missing billing_interval".to_owned(),
            )
        })?;
        let expected_amount_minor = session.amount_minor.ok_or_else(|| {
            VerifySubscriptionCheckoutError::Invalid(
                "subscription checkout session is missing amount_minor".to_owned(),
            )
        })?;
        let expected_currency = session
            .currency
            .as_deref()
            .ok_or_else(|| {
                VerifySubscriptionCheckoutError::Invalid(
                    "subscription checkout session is missing currency".to_owned(),
                )
            })
            .and_then(|s| {
                Currency::from_str(s).map_err(|e| {
                    VerifySubscriptionCheckoutError::Invalid(format!("invalid currency: {e}"))
                })
            })?;

        let verification = self
            .payment_provider
            .verify_payment_minor_units(&provider_reference)
            .await
            .map_err(VerifySubscriptionCheckoutError::Provider)?;

        match verification {
            SubscriptionPaymentVerification::Paid {
                provider_payment_id,
                provider_reference: verified_reference,
                amount_minor: got_amount_minor,
                currency: got_currency,
            } => {
                if got_currency != expected_currency {
                    return Err(VerifySubscriptionCheckoutError::CurrencyMismatch {
                        expected: expected_currency,
                        got: got_currency,
                    });
                }
                if got_amount_minor != expected_amount_minor {
                    return Err(VerifySubscriptionCheckoutError::AmountMismatch {
                        expected_minor: expected_amount_minor,
                        got_minor: got_amount_minor,
                    });
                }

                self.activate_or_record(
                    &session,
                    plan_tier,
                    billing_interval,
                    expected_currency,
                    expected_amount_minor,
                    &provider_payment_id,
                    &verified_reference,
                )
                .await?;

                let mut updated = session.clone();
                updated.mark_completed();
                updated.attach_provider_payment_id(provider_payment_id);
                self.checkout_session_repository
                    .update(&updated)
                    .await
                    .map_err(VerifySubscriptionCheckoutError::Repository)?;

                Ok(VerifySubscriptionCheckoutOutcome {
                    checkout_status: CheckoutStatus::Completed,
                })
            }
            SubscriptionPaymentVerification::Pending { .. } => {
                Ok(VerifySubscriptionCheckoutOutcome {
                    checkout_status: CheckoutStatus::Pending,
                })
            }
            SubscriptionPaymentVerification::Failed { .. } => {
                let mut updated = session.clone();
                updated.mark_failed();
                self.checkout_session_repository
                    .update(&updated)
                    .await
                    .map_err(VerifySubscriptionCheckoutError::Repository)?;
                Ok(VerifySubscriptionCheckoutOutcome {
                    checkout_status: CheckoutStatus::Failed,
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn activate_or_record(
        &self,
        session: &CheckoutSession,
        plan_tier: PlanTier,
        billing_interval: BillingInterval,
        currency: Currency,
        amount_minor: i64,
        provider_payment_id: &str,
        provider_reference: &str,
    ) -> Result<(), VerifySubscriptionCheckoutError> {
        let period_start = Utc::now();
        let period_end = period_end_from(period_start, billing_interval).map_err(|e| {
            VerifySubscriptionCheckoutError::Invalid(format!("period overflow: {e}"))
        })?;

        let existing = self
            .subscription_repository
            .find_by_tenant_id(session.tenant_id)
            .await
            .map_err(VerifySubscriptionCheckoutError::Repository)?;
        let subscription_id = existing
            .as_ref()
            .map(|s| s.id)
            .unwrap_or_else(SubscriptionId::new);

        let record = SubscriptionBillingRecord {
            id: uuid::Uuid::new_v4(),
            subscription_id: subscription_id.0,
            tenant_id: session.tenant_id.0,
            plan_tier,
            billing_interval,
            currency,
            amount_minor,
            status: BillingRecordStatus::Pending,
            provider: session.provider.clone(),
            provider_reference: provider_reference.to_owned(),
            provider_payment_id: Some(provider_payment_id.to_owned()),
            provider_subscription_code: None,
            period_start,
            period_end,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let outcome = self
            .subscription_billing_record_repository
            .try_insert(&record)
            .await
            .map_err(VerifySubscriptionCheckoutError::Repository)?;
        if outcome == InsertBillingRecordOutcome::AlreadyExists {
            return Ok(());
        }

        match existing {
            Some(mut sub) => {
                sub.apply_verified_payment_with_period(plan_tier, period_end)
                    .map_err(VerifySubscriptionCheckoutError::Subscription)?;
                self.subscription_repository
                    .update(&sub)
                    .await
                    .map_err(VerifySubscriptionCheckoutError::Repository)?;
            }
            None => {
                let sub = Subscription::new_with_period(
                    subscription_id,
                    session.tenant_id,
                    plan_tier,
                    period_end,
                );
                self.subscription_repository
                    .create(&sub)
                    .await
                    .map_err(VerifySubscriptionCheckoutError::Repository)?;
            }
        }

        self.subscription_billing_record_repository
            .mark_succeeded(
                &record.provider,
                &record.provider_reference,
                provider_payment_id,
            )
            .await
            .map_err(VerifySubscriptionCheckoutError::Repository)?;

        Ok(())
    }
}
