use crate::api::{client, monitoring};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MaintenanceWindow {
    pub id: String,
    pub monitor_id: String,
    pub starts_at: String,
    pub ends_at: String,
    pub reason: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EscalationPolicy {
    pub id: String,
    pub monitor_id: Option<String>,
    pub steps: Vec<EscalationStep>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EscalationStep {
    pub delay_minutes: u32,
    pub channel_id: String,
}

pub async fn list_maintenance(
    monitor_id: &str,
) -> Result<Vec<MaintenanceWindow>, client::ApiError> {
    client::get_json(&format!("/monitors/{monitor_id}/maintenance-windows")).await
}
pub async fn create_maintenance(
    monitor_id: &str,
    req: &monitoring::MaintenanceWindowPayload,
) -> Result<MaintenanceWindow, client::ApiError> {
    client::post_json(&format!("/monitors/{monitor_id}/maintenance-windows"), req).await
}
#[derive(Debug, Clone, Serialize)]
pub struct EscalationPolicyPayload {
    pub monitor_id: Option<String>,
    pub steps: Vec<EscalationStepPayload>,
}
#[derive(Debug, Clone, Serialize)]
pub struct EscalationStepPayload {
    pub delay_minutes: u32,
    pub channel_id: String,
}
pub async fn list_escalations() -> Result<Vec<EscalationPolicy>, client::ApiError> {
    client::get_json("/escalation-policies").await
}
pub async fn create_escalation(
    req: &EscalationPolicyPayload,
) -> Result<EscalationPolicy, client::ApiError> {
    client::post_json("/escalation-policies", req).await
}
pub async fn update_escalation(
    id: &str,
    req: &EscalationPolicyPayload,
) -> Result<EscalationPolicy, client::ApiError> {
    client::put_json(&format!("/escalation-policies/{id}"), req).await
}
