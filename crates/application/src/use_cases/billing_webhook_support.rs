use std::{error::Error, fmt};

#[derive(Debug)]
pub enum BillingWebhookError {
    SubscriptionNotFound,
    MissingPlanTier,
    InvalidTransition(domain::DomainError),
    Repository(Box<dyn Error + Send + Sync>),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for BillingWebhookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubscriptionNotFound => write!(f, "no subscription found for this tenant"),
            Self::MissingPlanTier => write!(f, "plan tier is required for this event"),
            Self::InvalidTransition(err) => write!(f, "{err}"),
            Self::Repository(err) => write!(f, "{err}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for BillingWebhookError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(err) => Some(err.as_ref()),
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<Box<dyn Error + Send + Sync>> for BillingWebhookError {
    fn from(err: Box<dyn Error + Send + Sync>) -> Self {
        Self::Repository(err)
    }
}

impl From<domain::DomainError> for BillingWebhookError {
    fn from(err: domain::DomainError) -> Self {
        Self::InvalidTransition(err)
    }
}
