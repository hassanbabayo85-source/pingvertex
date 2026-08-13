use serde::{Deserialize, Serialize};

/// The delivery mechanism for a `NotificationChannel`.
///
/// Only `Email` and `Webhook` are implemented today. Telegram/Discord/Slack
/// were listed as future work in the original `notification` module stub —
/// adding one later means adding a variant here plus one new adapter in
/// `crates/infra/src/services/`, without touching the domain entity shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannelKind {
    Email,
    Webhook,
}

impl std::fmt::Display for NotificationChannelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Email => write!(f, "email"),
            Self::Webhook => write!(f, "webhook"),
        }
    }
}
