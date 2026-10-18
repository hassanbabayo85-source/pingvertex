use serde::{Deserialize, Serialize};

/// Provider-agnostic billing webhook event type.
///
/// These are the only webhook events understands. The provider's
/// raw event string is mapped into this enum before application logic
/// runs, so no provider-specific casing or type names leak inward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingWebhookEventType {
    /// A one-off or recurring payment succeeded.
    PaymentSucceeded,
    /// A payment failed or was declined.
    PaymentFailed,
    /// A subscription renewed successfully (treated as a successful
    /// payment for lifecycle purposes).
    SubscriptionRenewed,
    /// The provider canceled the subscription.
    SubscriptionCanceled,
    /// A provider-confirmed refund.
    PaymentRefunded,
}

impl BillingWebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaymentSucceeded => "payment_succeeded",
            Self::PaymentFailed => "payment_failed",
            Self::SubscriptionRenewed => "subscription_renewed",
            Self::SubscriptionCanceled => "subscription_canceled",
            Self::PaymentRefunded => "payment_refunded",
        }
    }
}

impl std::str::FromStr for BillingWebhookEventType {
    type Err = crate::errors::DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "payment_succeeded" => Ok(Self::PaymentSucceeded),
            "payment_failed" => Ok(Self::PaymentFailed),
            "subscription_renewed" => Ok(Self::SubscriptionRenewed),
            "subscription_canceled" => Ok(Self::SubscriptionCanceled),
            "payment_refunded" => Ok(Self::PaymentRefunded),
            other => Err(crate::errors::DomainError::InvalidBillingWebhookEventType(
                other.to_owned(),
            )),
        }
    }
}
