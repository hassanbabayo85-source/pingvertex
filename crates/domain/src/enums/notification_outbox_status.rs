use serde::{Deserialize, Serialize};

/// Lifecycle state for a notification outbox row.
///
/// The worker claims only `Pending` rows whose `retry_at` is due. A
/// temporary delivery failure moves the row back to `Pending` with a
/// future `retry_at`; `Failed` is reserved for rows that exhausted their
/// configured maximum attempt count and now require operator attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationOutboxStatus {
    Pending,
    Processing,
    Sent,
    Failed,
}

impl NotificationOutboxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for NotificationOutboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
