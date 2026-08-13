use async_trait::async_trait;
use domain::{CheckId, IncidentId, MonitorId, NotificationChannelId, UserId};

/// Generates unique IDs for domain entities.
#[async_trait]
pub trait IdGenerator: Send + Sync {
    async fn next_user_id(&self) -> UserId;

    async fn next_monitor_id(&self) -> MonitorId;

    async fn next_check_id(&self) -> CheckId;

    async fn next_incident_id(&self) -> IncidentId;

    async fn next_notification_channel_id(&self) -> NotificationChannelId;

    /// Generates a new high-entropy secret used to HMAC-sign outgoing
    /// webhook payloads (see `crates/infra/src/services/webhook_notification_service.rs`).
    ///
    /// Placed on this port rather than introducing a dedicated
    /// "SecretGenerator" port: both are "give me cryptographically random
    /// bytes for a new entity" concerns, this project already threads
    /// `IdGenerator` through every write path that needs one, and adding a
    /// second near-identical port for a single extra method would be more
    /// ceremony than it's worth at this scale. Not an ID in the strict
    /// sense, but the same "I generate randomness, infra decides how" role.
    async fn next_webhook_signing_secret(&self) -> String;
}
