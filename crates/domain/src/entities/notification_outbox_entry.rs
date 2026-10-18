use chrono::{DateTime, Utc};

use crate::{
    enums::{NotificationOutboxEventKind, NotificationOutboxStatus},
    shared::ids::{NotificationChannelId, NotificationOutboxEntryId},
};

/// One reliably-delivered notification waiting in the outbox.
///
/// `payload` is a JSON string containing the event payload. defines
/// the payload schema as a serialized `Monitor` snapshot:
/// `serde_json::to_string(&monitor)`. The sender deserializes it back
/// into a `Monitor` and reconstructs the matching `NotificationEvent`
/// from `event_kind`.
#[derive(Debug, Clone)]
pub struct NotificationOutboxEntry {
    pub id: NotificationOutboxEntryId,
    pub channel_id: NotificationChannelId,
    pub event_kind: NotificationOutboxEventKind,
    /// JSON-serialized `Monitor` snapshot (sender).
    pub payload: String,
    pub idempotency_key: String,
    pub status: NotificationOutboxStatus,
    pub attempts: i32,
    pub retry_at: DateTime<Utc>,
    pub processing_started_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NotificationOutboxEntry {
    pub fn new(
        id: NotificationOutboxEntryId,
        channel_id: NotificationChannelId,
        event_kind: NotificationOutboxEventKind,
        payload: String,
        idempotency_key: String,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            channel_id,
            event_kind,
            payload,
            idempotency_key,
            status: NotificationOutboxStatus::Pending,
            attempts: 0,
            retry_at: created_at,
            processing_started_at: None,
            error: None,
            created_at,
            updated_at: created_at,
        }
    }
}
