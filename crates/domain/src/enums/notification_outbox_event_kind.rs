use serde::{Deserialize, Serialize};

/// The lifecycle event a queued notification is about.
///
/// Mirrors `crate::notification::NotificationEvent`, but deliberately
/// keeps only the serializable discriminant so the outbox can persist
/// events without coupling itself to the full `Monitor` entity in this
/// domain layer. The associated monitor snapshot is carried separately
/// in `NotificationOutboxEntry::payload` as a JSON string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationOutboxEventKind {
    MonitorDown,
    MonitorRecovered,
    Test,
}

impl NotificationOutboxEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MonitorDown => "monitor_down",
            Self::MonitorRecovered => "monitor_recovered",
            Self::Test => "test",
        }
    }
}

impl std::fmt::Display for NotificationOutboxEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
