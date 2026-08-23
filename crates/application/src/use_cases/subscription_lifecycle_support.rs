use std::error::Error;
use std::fmt;

/// Shared error type for subscription lifecycle transition use cases
/// (`BeginGracePeriodUseCase`, `ExpireSubscriptionUseCase`,
/// `CancelSubscriptionUseCase`, `FinalizeScheduledCancellationUseCase`).
#[derive(Debug)]
pub enum SubscriptionLifecycleError {
    /// No subscription row exists for this tenant. A tenant with no
    /// subscription is implicitly on the Free plan (see
    /// `ResolveEntitlementsUseCase`) and is not a valid subject for a
    /// lifecycle transition — there is nothing to transition.
    SubscriptionNotFound,
    /// The requested transition is not valid from the subscription's
    /// current state.
    InvalidTransition(domain::DomainError),
    Repository(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for SubscriptionLifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubscriptionNotFound => write!(f, "no subscription found for this tenant"),
            Self::InvalidTransition(err) => write!(f, "{err}"),
            Self::Repository(err) => write!(f, "{err}"),
        }
    }
}

impl Error for SubscriptionLifecycleError {}

impl From<Box<dyn Error + Send + Sync>> for SubscriptionLifecycleError {
    fn from(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::Repository(err)
    }
}

impl From<domain::DomainError> for SubscriptionLifecycleError {
    fn from(err: domain::DomainError) -> Self {
        Self::InvalidTransition(err)
    }
}
