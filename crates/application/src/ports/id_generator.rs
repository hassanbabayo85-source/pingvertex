use async_trait::async_trait;
use domain::{
    ApiFlowId, CheckId, DiagnosticReportId, EscalationPolicyId, IncidentId, MaintenanceWindowId,
    MonitorId, NotificationChannelId, ProxyCheckJobId, ProxyEntryId, StatusPageId, UserId,
};

/// Generates unique IDs for domain entities.
#[async_trait]
pub trait IdGenerator: Send + Sync {
    async fn next_user_id(&self) -> UserId;

    async fn next_monitor_id(&self) -> MonitorId;

    async fn next_check_id(&self) -> CheckId;

    async fn next_incident_id(&self) -> IncidentId;

    async fn next_notification_channel_id(&self) -> NotificationChannelId;

    async fn next_api_flow_id(&self) -> ApiFlowId;

    /// Generates a new high-entropy secret used to HMAC-sign outgoing
    /// webhook payloads (see `crates/infra/src/services/webhook_notification_service.rs`).
    async fn next_webhook_signing_secret(&self) -> String;

    /// Generates a new `DiagnosticReportId`.
    ///
    /// Given a default implementation (rather than a required method)
    /// because, like every other ID in this trait, it is just a fresh
    /// random UUID wrapper with no infra-specific state involved —
    /// existing implementors (e.g. `UuidIdGenerator`) do not need to be
    /// touched to pick this up.
    async fn next_diagnostic_report_id(&self) -> DiagnosticReportId {
        DiagnosticReportId::new()
    }

    /// Generates a new `EscalationPolicyId`.
    async fn next_escalation_policy_id(&self) -> EscalationPolicyId {
        EscalationPolicyId::new()
    }

    /// Generates a new `MaintenanceWindowId`.
    async fn next_maintenance_window_id(&self) -> MaintenanceWindowId {
        MaintenanceWindowId::new()
    }

    /// Generates a new `StatusPageId`.
    ///
    /// Same reasoning as the other default-impl IDs above: a fresh random
    /// UUID wrapper, no infra-specific state, so existing implementors of
    /// this trait pick it up for free.
    async fn next_status_page_id(&self) -> StatusPageId {
        StatusPageId::new()
    }

    /// Generates a new `ProxyCheckJobId`.
    ///
    /// Same reasoning as `next_status_page_id`: a fresh random UUID
    /// wrapper with no infra-specific state, so existing implementors of
    /// this trait (e.g. `UuidIdGenerator`) pick it up for free without
    /// needing to be touched.
    async fn next_proxy_check_job_id(&self) -> ProxyCheckJobId {
        ProxyCheckJobId::new()
    }

    /// Generates a new `ProxyEntryId`.
    ///
    /// Same reasoning as `next_proxy_check_job_id` above.
    async fn next_proxy_entry_id(&self) -> ProxyEntryId {
        ProxyEntryId::new()
    }
}
