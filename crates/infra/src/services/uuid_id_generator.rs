use application::ports::IdGenerator;
use async_trait::async_trait;
use domain::{ApiFlowId, CheckId, IncidentId, MonitorId, NotificationChannelId, UserId};
use rand_core::{OsRng, RngCore};

#[derive(Debug, Default, Clone)]
pub struct UuidIdGenerator;

impl UuidIdGenerator {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl IdGenerator for UuidIdGenerator {
    async fn next_user_id(&self) -> UserId {
        UserId::generate()
    }

    async fn next_monitor_id(&self) -> MonitorId {
        MonitorId::generate()
    }

    async fn next_check_id(&self) -> CheckId {
        CheckId::generate()
    }

    async fn next_incident_id(&self) -> IncidentId {
        IncidentId::generate()
    }

    async fn next_notification_channel_id(&self) -> NotificationChannelId {
        NotificationChannelId::generate()
    }

    async fn next_api_flow_id(&self) -> ApiFlowId {
        ApiFlowId::new()
    }

    async fn next_webhook_signing_secret(&self) -> String {
        // 32 bytes (256 bits) from the OS CSPRNG, hex-encoded, with a
        // "whsec_" prefix so the secret is self-describing at a glance
        // (same convention Stripe/GitHub use for their webhook secrets) —
        // this is what gets shown to the user exactly once on channel
        // creation and used as the HMAC-SHA256 key in
        // `crates/infra/src/services/webhook_notification_service.rs`.
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        format!("whsec_{}", hex::encode(bytes))
    }
}
