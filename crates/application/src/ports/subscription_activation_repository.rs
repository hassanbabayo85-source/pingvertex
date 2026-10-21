//! BE-1 F5 — atomic subscription activation port.
//!
//! Single entry point that atomically persists every write required to
//! turn a verified Paystack payment into an active subscription:
//!
//!   1. insert-or-find the `subscription_billing_records` row,
//!   2. create or update the tenant's `subscriptions` row,
//!   3. mark the billing record `succeeded`, and
//!   4. mark the `checkout_sessions` row `completed`.
//!
//! The pre-F5 flow performed these steps in the application layer
//! across four separate repository calls, so a crash between any two of
//! them left the system in a partial state. F5 collapses the four writes
//! into one `sqlx::Transaction` inside the Postgres impl.
//!
//! `provider.verify_payment_minor_units` is **deliberately outside** this
//! port — it is a network call and cannot participate in a SQL
//! transaction. The caller verifies first, then constructs the
//! `SubscriptionBillingRecord` and the `SubscriptionAction` from the
//! verified result and hands them to `activate_atomically`.

use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

use domain::Subscription;
use domain::shared::ids::CheckoutSessionId;

use super::subscription_billing_record_repository::SubscriptionBillingRecord;

/// Typed error returned by `SubscriptionActivationRepository::activate_atomically`.
///
/// Variants split into two classes:
///
/// * **Client-facing (bad-request) errors** — a business invariant failed:
///   a billing record in `Failed` state, a mismatched identity, a terminal
///   subscription referenced by a `Succeeded` record, a bad session id, etc.
///   The use case maps these to `VerifySubscriptionCheckoutError::Invalid`
///   so the HTTP layer returns 400.
/// * **Server-side errors** — a database call failed, or a row-count
///   invariant that should always hold (exactly 1 row) was violated. The
///   use case maps these to `VerifySubscriptionCheckoutError::Repository`
///   so the HTTP layer returns 500.
///
/// `is_client_error` centralizes the split; a caller that adds a new
/// variant MUST decide which class it belongs to and update the match.
#[derive(Debug)]
pub enum SubscriptionActivationError {
    // ── Client-facing business-invariant errors (→ 400) ────────
    BillingRecordFailed,
    BillingRecordTenantMismatch,
    BillingRecordPlanTierMismatch,
    BillingRecordIntervalMismatch,
    BillingRecordCurrencyMismatch,
    BillingRecordAmountMismatch,
    BillingRecordMissingAfterConflict,
    BillingRecordStatusUnknown(String),
    SucceededSubscriptionMissing,
    SucceededSubscriptionTenantMismatch,
    SucceededSubscriptionTerminal,
    ConcurrentWinnerTenantMismatch,
    ConcurrentWinnerPlanMismatch,
    ConcurrentWinnerTerminal,
    UpdateOnCanceledSubscription,
    CheckoutSessionNotFound,
    CheckoutSessionTenantMismatch,
    CheckoutSessionAlreadyFailed,
    CheckoutSessionUpdateRejected,
    MissingProviderPaymentId,

    // ── Server-side errors (→ 500) ─────────────────────────────
    SubscriptionUpdateMatchedNoRows,
    BillingRecordLinkFailed,
    BillingRecordMarkSucceededFailed,

    Database(Box<dyn Error + Send + Sync>),
}

impl std::fmt::Display for SubscriptionActivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BillingRecordFailed => write!(
                f,
                "existing billing record is in failed state; cannot reactivate"
            ),
            Self::BillingRecordTenantMismatch => {
                write!(f, "existing billing record belongs to a different tenant")
            }
            Self::BillingRecordPlanTierMismatch => write!(
                f,
                "existing billing record does not match this checkout (plan_tier)"
            ),
            Self::BillingRecordIntervalMismatch => write!(
                f,
                "existing billing record does not match this checkout (billing_interval)"
            ),
            Self::BillingRecordCurrencyMismatch => write!(
                f,
                "existing billing record does not match this checkout (currency)"
            ),
            Self::BillingRecordAmountMismatch => write!(
                f,
                "existing billing record does not match this checkout (amount_minor)"
            ),
            Self::BillingRecordMissingAfterConflict => write!(
                f,
                "billing record reported as already existing but not found"
            ),
            Self::BillingRecordStatusUnknown(s) => {
                write!(f, "unknown billing record status: {s}")
            }
            Self::SucceededSubscriptionMissing => write!(
                f,
                "existing Succeeded billing record references a missing subscription"
            ),
            Self::SucceededSubscriptionTenantMismatch => write!(
                f,
                "existing Succeeded billing record's subscription belongs to a different tenant"
            ),
            Self::SucceededSubscriptionTerminal => write!(
                f,
                "existing Succeeded billing record's subscription is in terminal state \
                 (canceled or expired); a fresh checkout is required to reactivate"
            ),
            Self::ConcurrentWinnerTenantMismatch => write!(
                f,
                "concurrent winner subscription has an unexpected tenant_id"
            ),
            Self::ConcurrentWinnerPlanMismatch => write!(
                f,
                "concurrent winner subscription has a different plan_tier"
            ),
            Self::ConcurrentWinnerTerminal => write!(
                f,
                "concurrent winner subscription is in terminal state (canceled or expired)"
            ),
            Self::UpdateOnCanceledSubscription => write!(
                f,
                "cannot persist a subscription UPDATE on a canceled subscription"
            ),
            Self::CheckoutSessionNotFound => write!(f, "checkout session not found"),
            Self::CheckoutSessionTenantMismatch => {
                write!(f, "checkout session belongs to a different tenant")
            }
            Self::CheckoutSessionAlreadyFailed => {
                write!(f, "checkout session is already in failed state")
            }
            Self::CheckoutSessionUpdateRejected => write!(
                f,
                "checkout session could not be marked completed \
                 (missing, tenant mismatch, or already failed)"
            ),
            Self::MissingProviderPaymentId => write!(
                f,
                "billing record must have provider_payment_id at activation time"
            ),
            Self::SubscriptionUpdateMatchedNoRows => {
                write!(f, "subscription UPDATE matched zero rows")
            }
            Self::BillingRecordLinkFailed => write!(
                f,
                "failed to link billing record to the persisted subscription"
            ),
            Self::BillingRecordMarkSucceededFailed => {
                write!(f, "failed to mark billing record as succeeded")
            }
            Self::Database(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SubscriptionActivationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl SubscriptionActivationError {
    /// True if this error class is caused by a client-visible business
    /// invariant (bad request), not a database / internal failure.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::BillingRecordFailed
                | Self::BillingRecordTenantMismatch
                | Self::BillingRecordPlanTierMismatch
                | Self::BillingRecordIntervalMismatch
                | Self::BillingRecordCurrencyMismatch
                | Self::BillingRecordAmountMismatch
                | Self::BillingRecordMissingAfterConflict
                | Self::BillingRecordStatusUnknown(_)
                | Self::SucceededSubscriptionMissing
                | Self::SucceededSubscriptionTenantMismatch
                | Self::SucceededSubscriptionTerminal
                | Self::ConcurrentWinnerTenantMismatch
                | Self::ConcurrentWinnerPlanMismatch
                | Self::ConcurrentWinnerTerminal
                | Self::UpdateOnCanceledSubscription
                | Self::CheckoutSessionNotFound
                | Self::CheckoutSessionTenantMismatch
                | Self::CheckoutSessionAlreadyFailed
                | Self::CheckoutSessionUpdateRejected
                | Self::MissingProviderPaymentId
        )
    }
}

impl From<Box<dyn Error + Send + Sync>> for SubscriptionActivationError {
    fn from(e: Box<dyn Error + Send + Sync>) -> Self {
        Self::Database(e)
    }
}

pub type SubscriptionActivationResult<T> = Result<T, SubscriptionActivationError>;

/// What the caller wants to do to the tenant's subscription as part of
/// this activation.
#[derive(Debug, Clone)]
pub enum SubscriptionAction {
    /// No subscription existed at the caller's pre-query time; create a
    /// new one.
    ///
    /// If another request wins the `subscriptions (tenant_id)` UNIQUE
    /// race between the caller's pre-query and the port's transaction,
    /// the port MUST accept the persisted winner if it is compatible
    /// (same `plan_tier`, status not `Canceled`/`Expired`) and return
    /// `ActivationOutcome::ConcurrentWinnerAccepted`. The persisted
    /// billing record's `subscription_id` MUST then reference the
    /// winner's `id`, not the loser's pre-generated `id`.
    Create(Subscription),
    /// An existing subscription existed at the caller's pre-query time;
    /// update it.
    ///
    /// The caller is expected to have already applied
    /// `Subscription::apply_verified_payment_with_period` to the value
    /// inside this variant. If the subscription disappears between the
    /// caller's pre-query and the port's transaction, the port MUST
    /// return `Err(SubscriptionUpdateMatchedNoRows)`.
    Update(Subscription),
}

/// Successful outcomes of an atomic activation. Every variant implies the
/// billing record is `succeeded`, the tenant has an active subscription,
/// the checkout session is `completed`, and all writes committed in one
/// transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    Created { subscription_id: uuid::Uuid },
    Updated { subscription_id: uuid::Uuid },
    ConcurrentWinnerAccepted { subscription_id: uuid::Uuid },
    AlreadySucceeded { subscription_id: uuid::Uuid },
}

/// Atomic subscription activation port.
///
/// See the module-level docs for the transaction boundary and the
/// per-variant outcome contract. `activate_atomically` MUST execute every
/// write in a single database transaction. Any error returned MUST roll
/// back the whole transaction.
#[async_trait]
pub trait SubscriptionActivationRepository: Send + Sync {
    /// Atomically activate a verified subscription payment.
    ///
    /// # Failure classes
    ///
    /// `SubscriptionActivationError::is_client_error` distinguishes:
    ///
    /// * **Client (bad request)** — Failed record, identity mismatch,
    ///   terminal subscription on `Succeeded`, bad session id, etc.
    /// * **Server** — database error, or a "should always be 1 row"
    ///   invariant failed.
    ///
    /// The current Postgres and InMemory implementations of this trait
    /// document their exact behavior against the Phase 2/3/4 trait
    /// contract; see the module docs of those files.
    async fn activate_atomically(
        &self,
        record: SubscriptionBillingRecord,
        action: SubscriptionAction,
        session_id: CheckoutSessionId,
    ) -> SubscriptionActivationResult<ActivationOutcome>;
}

#[async_trait]
impl<T> SubscriptionActivationRepository for Arc<T>
where
    T: SubscriptionActivationRepository + ?Sized,
{
    async fn activate_atomically(
        &self,
        record: SubscriptionBillingRecord,
        action: SubscriptionAction,
        session_id: CheckoutSessionId,
    ) -> SubscriptionActivationResult<ActivationOutcome> {
        (**self)
            .activate_atomically(record, action, session_id)
            .await
    }
}
