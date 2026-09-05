#![allow(dead_code)]

use crate::api::client::{get_json, get_json_query, post_json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ProxyInput {
    pub host: String,
    pub port: u16,
    pub scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateProxyCheckRequest {
    pub proxies: Vec<ProxyInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProxyJob {
    pub id: String,
    pub status: String,
    pub total_count: u32,
    pub checked_count: u32,
    pub successful_count: u32,
    pub failed_count: u32,
    pub started_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub completed_at: Option<String>,
    pub failed_at: Option<String>,
    pub canceled_at: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyResult {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub scheme: String,
    pub result: Option<ProxyResultBody>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyResultBody {
    pub reachable: bool,
    pub latency_ms: Option<u32>,
    pub supports_https: bool,
    pub country_code: Option<String>,
    pub anonymity: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ProxyResults {
    pub items: Vec<ProxyResult>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

pub async fn create(
    req: &CreateProxyCheckRequest,
) -> Result<ProxyJob, crate::api::client::ApiError> {
    post_json("/proxy-checks", req).await
}
pub async fn get(id: &str) -> Result<ProxyJob, crate::api::client::ApiError> {
    get_json(&format!("/proxy-checks/{id}")).await
}
pub async fn results(
    id: &str,
    limit: u32,
    offset: u32,
    working_only: bool,
) -> Result<ProxyResults, crate::api::client::ApiError> {
    get_json_query(
        &format!("/proxy-checks/{id}/results"),
        &[
            ("limit", &limit.to_string()),
            ("offset", &offset.to_string()),
            ("working_only", if working_only { "true" } else { "false" }),
        ],
    )
    .await
}
pub async fn cancel(id: &str) -> Result<ProxyJob, crate::api::client::ApiError> {
    post_json(
        &format!("/proxy-checks/{id}/cancel"),
        &serde_json::json!({}),
    )
    .await
}
