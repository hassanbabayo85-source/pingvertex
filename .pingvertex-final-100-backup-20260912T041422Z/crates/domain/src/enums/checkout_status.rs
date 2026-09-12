use serde::{Deserialize, Serialize};

/// Lifecycle status for a provider-agnostic checkout session.
///
/// `Pending` means the provider has not yet confirmed payment.
/// `Completed` means server-side verification succeeded and the tenant's
/// subscription has been activated. `Failed` means the provider explicitly
/// reported that payment did not succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckoutStatus {
    Pending,
    Completed,
    Failed,
}

impl CheckoutStatus {
    /// Stable machine-readable string, used as the `status` column value
    /// in `checkout_sessions`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for CheckoutStatus {
    type Err = crate::errors::DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            other => Err(crate::errors::DomainError::InvalidCheckoutStatus(
                other.to_owned(),
            )),
        }
    }
}
