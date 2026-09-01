#![allow(dead_code)]
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use web_sys::RequestCredentials;

const API_BASE: &str = "/api/v1";

async fn send_request(method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    let url = format!("{}{}", API_BASE, path);
    let builder = match method {
        "GET" => Request::get(&url),
        "POST" => Request::post(&url),
        "PUT" => Request::put(&url),
        "DELETE" => Request::delete(&url),
        _ => return Err(format!("Unsupported method {method}")),
    }
    .header("Content-Type", "application/json")
    .credentials(RequestCredentials::Include);

    let request = if let Some(b) = body {
        let json_str = serde_json::to_string(&b).map_err(|e| e.to_string())?;
        builder.body(json_str).map_err(|e| e.to_string())?
    } else {
        builder.build().map_err(|e| e.to_string())?
    };

    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON response: {e}"))
    } else {
        Err(format!("Request failed with status {status}: {text}"))
    }
}

async fn get_json(path: &str) -> Result<Value, String> {
    send_request("GET", path, None).await
}

async fn post_json(path: &str, body: Value) -> Result<Value, String> {
    send_request("POST", path, Some(body)).await
}

async fn put_json(path: &str, body: Value) -> Result<Value, String> {
    send_request("PUT", path, Some(body)).await
}

async fn delete_json(path: &str) -> Result<Value, String> {
    send_request("DELETE", path, None).await
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub enabled: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub status: Option<String>,
    pub uptime_percentage: Option<f64>,
    pub avg_response_time_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Overview {
    pub total_monitors: u64,
    pub up_monitors: u64,
    pub down_monitors: u64,
    pub uptime_percentage: f64,
    pub average_response_time_ms: f64,
    pub active_incidents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Check {
    pub id: String,
    pub monitor_id: String,
    pub status: String,
    pub response_time_ms: Option<u64>,
    pub checked_at: String,
    pub region: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Incident {
    pub id: String,
    pub monitor_id: String,
    pub status: String,
    pub title: String,
    pub description: Option<String>,
    pub started_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub target: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonitorPayload {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelPayload {
    pub name: String,
    pub kind: String,
    pub target: String,
}

pub async fn fetch_monitors() -> Result<Vec<Monitor>, String> {
    let value = get_json("/monitors").await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid monitors data: {e}"))
}

pub async fn get_monitor(id: &str) -> Result<Monitor, String> {
    let value = get_json(&format!("/monitors/{id}")).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid monitor data: {e}"))
}

pub async fn create_monitor(payload: &MonitorPayload) -> Result<Monitor, String> {
    let body = serde_json::to_value(payload).map_err(|e| e.to_string())?;
    let value = post_json("/monitors", body).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid monitor data: {e}"))
}

pub async fn update_monitor(id: &str, payload: &MonitorPayload) -> Result<Monitor, String> {
    let body = serde_json::to_value(payload).map_err(|e| e.to_string())?;
    let value = put_json(&format!("/monitors/{id}"), body).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid monitor data: {e}"))
}

pub async fn delete_monitor(id: &str) -> Result<(), String> {
    delete_json(&format!("/monitors/{id}")).await?;
    Ok(())
}

pub async fn set_monitor_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let path = if enabled {
        format!("/monitors/{id}/enable")
    } else {
        format!("/monitors/{id}/disable")
    };
    post_json(&path, json!({})).await?;
    Ok(())
}

pub async fn fetch_overview() -> Result<Overview, String> {
    let value = get_json("/dashboard/overview").await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid overview data: {e}"))
}

pub async fn fetch_incidents(monitor_id: &str) -> Result<Vec<Incident>, String> {
    let value = get_json(&format!("/monitors/{monitor_id}/incidents")).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid incidents data: {e}"))
}

pub async fn fetch_checks(monitor_id: &str) -> Result<Vec<Check>, String> {
    let value = get_json(&format!("/monitors/{monitor_id}/checks")).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid checks data: {e}"))
}

pub async fn list_notification_channels(
    monitor_id: &str,
) -> Result<Vec<NotificationChannel>, String> {
    let value = get_json(&format!("/monitors/{monitor_id}/channels")).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid channels data: {e}"))
}

pub async fn create_notification_channel(
    monitor_id: &str,
    payload: &ChannelPayload,
) -> Result<NotificationChannel, String> {
    let body = serde_json::to_value(payload).map_err(|e| e.to_string())?;
    let value = post_json(&format!("/monitors/{monitor_id}/channels"), body).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid channel data: {e}"))
}

pub async fn delete_notification_channel(channel_id: &str) -> Result<(), String> {
    delete_json(&format!("/channels/{channel_id}")).await?;
    Ok(())
}
