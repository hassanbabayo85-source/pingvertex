use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreatedApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiFlow {
    pub id: String,
    pub monitor_id: String,
    pub steps: Vec<ApiFlowStep>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiFlowStep {
    pub order: u16,
    pub name: String,
    pub method: String,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub extract: Vec<Extraction>,
    pub assertions: Vec<Assertion>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Extraction {
    StatusCode,
    Header { name: String },
    JsonPath { path: String },
    Body,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Assertion {
    pub id: String,
    pub response_part: String,
    pub operator: String,
    pub target: Option<String>,
    pub expected_value: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiFlowRequest {
    pub steps: Vec<ApiFlowStepRequest>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ApiFlowStepRequest {
    pub order: u16,
    pub name: String,
    pub method: String,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub extract: Vec<ExtractionRequest>,
    pub assertions: Vec<AssertionRequest>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtractionRequest {
    StatusCode,
    Header { name: String },
    JsonPath { path: String },
    Body,
}
#[derive(Debug, Clone, Serialize)]
pub struct AssertionRequest {
    pub response_part: String,
    pub operator: String,
    pub target: Option<String>,
    pub expected_value: Option<String>,
}

pub const API_KEY_SCOPES: &[&str] = &[
    "monitor:read",
    "monitor:write",
    "status_page:read",
    "status_page:write",
    "incident:read",
    "incident:write",
    "notification:read",
    "notification:write",
    "api_flow:read",
    "api_flow:write",
];

pub async fn list_keys() -> Result<Vec<ApiKey>, ApiError> {
    client::get_json("/api-keys").await
}
pub async fn create_key(request: &CreateApiKeyRequest) -> Result<CreatedApiKey, ApiError> {
    client::post_json("/api-keys", request).await
}
pub async fn delete_key(id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/api-keys/{id}")).await
}
pub async fn list_flows(monitor_id: &str) -> Result<Vec<ApiFlow>, ApiError> {
    client::get_json(&format!("/monitors/{monitor_id}/flows")).await
}
pub async fn create_flow(
    monitor_id: &str,
    request: &CreateApiFlowRequest,
) -> Result<ApiFlow, ApiError> {
    client::post_json(&format!("/monitors/{monitor_id}/flows"), request).await
}
pub async fn update_flow(
    monitor_id: &str,
    flow_id: &str,
    request: &CreateApiFlowRequest,
) -> Result<ApiFlow, ApiError> {
    client::put_json(&format!("/monitors/{monitor_id}/flows/{flow_id}"), request).await
}

pub async fn delete_flow(monitor_id: &str, flow_id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/monitors/{monitor_id}/flows/{flow_id}")).await
}
