// =====================================================
// Notification Domain
//
// Owns:
// - Email
// - Webhook
// - Telegram (future)
// - Discord (future)
// - Slack (future)
//
// The channel itself (`NotificationChannel`, holding the kind/target/
// signing secret) lives in `crate::entities`, alongside this project's
// other persisted entities (`Monitor`, `Incident`, ...), to stay
// consistent with the existing convention in this codebase rather than
// introducing a second "entities" location.
//
// What THIS module owns is the *event* that triggers a notification --
// the thing every channel, regardless of kind, needs to know about in
// order to render a message.
// =====================================================

use crate::entities::Monitor;

/// A monitor lifecycle event that should be delivered to a monitor's
/// notification channels.
///
/// `Test` is deliberately a distinct variant (not reused as `MonitorDown`)
/// so a "send test notification" request can never be confused with an
/// actual outage by anything reading this enum downstream (email subject
/// lines, webhook payloads, logs) -- see
/// `crates/application/src/use_cases/send_test_notification.rs`.
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    MonitorDown(Monitor),
    MonitorRecovered(Monitor),
    Test(Monitor),
}

impl NotificationEvent {
    /// Returns the monitor this event is about, regardless of variant.
    pub fn monitor(&self) -> &Monitor {
        match self {
            Self::MonitorDown(monitor) | Self::MonitorRecovered(monitor) | Self::Test(monitor) => {
                monitor
            }
        }
    }
}
