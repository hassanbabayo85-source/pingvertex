use std::{error::Error, fmt};

use crate::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;

/// Shared error type for B14 billing use cases.
#[derive(Debug)]
pub enum BillingError {
    /// Checkout was requested for the Free plan. Free requires no checkout.
    FreePlanNotAllowed,
    /// Client-supplied or persisted data is internally inconsistent (e.g.
    /// a subscription checkout session with no plan tier). This is not a
    /// user error and not a provider error — the session itself is bad.
    Invalid(String),
    Provider(Box<dyn Error + Send + Sync>),
    CheckoutSessionNotFound,
    /// The session exists but has no provider checkout id attached yet.
    CheckoutNotReady,
    Subscription(SubscriptionLifecycleError),
}

impl fmt::Display for BillingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FreePlanNotAllowed => write!(f, "free plan does not require checkout"),
            Self::Invalid(msg) => write!(f, "{msg}"),
            Self::Provider(err) => write!(f, "{err}"),
            Self::CheckoutSessionNotFound => write!(f, "checkout session not found"),
            Self::CheckoutNotReady => write!(f, "checkout session is not ready to verify"),
            Self::Subscription(err) => write!(f, "{err}"),
        }
    }
}

impl Error for BillingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(err) => Some(err.as_ref()),
            Self::Subscription(err) => Some(err),
            _ => None,
        }
    }
}

impl From<Box<dyn Error + Send + Sync>> for BillingError {
    fn from(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::Provider(err)
    }
}

impl From<SubscriptionLifecycleError> for BillingError {
    fn from(err: SubscriptionLifecycleError) -> Self {
        Self::Subscription(err)
    }
}
