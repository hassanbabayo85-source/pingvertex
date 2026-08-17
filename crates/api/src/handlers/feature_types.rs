use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Request body for `POST /monitors/{monitor_id}/maintenance-windows`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMaintenanceWindowRequest {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub reason: String,
}

/// A single escalation step as supplied by the API client. `channel_id` is
/// a string here; the handler parses it to `NotificationChannelId`.
#[derive(Debug, Clone, Deserialize)]
pub struct EscalationStepInput {
    pub delay_minutes: u32,
    pub channel_id: String,
}

/// Request body for `POST /escalation-policies`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateEscalationPolicyRequest {
    /// Optional; when absent, creates a global policy for the user.
    pub monitor_id: Option<String>,
    pub steps: Vec<EscalationStepInput>,
}
