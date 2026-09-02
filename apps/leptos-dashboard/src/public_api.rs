use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

const API_BASE: &str = "/api/v1/public";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicMonitor {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub status: String,
    pub uptime: f64,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Incident {
    pub id: i64,
    pub monitor_id: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicStatusResponse {
    pub overall_status: String,
    pub monitors: Vec<PublicMonitor>,
    pub incidents: Vec<Incident>,
}

pub async fn get_public_status() -> Result<PublicStatusResponse, String> {
    let url = format!("{}/status", API_BASE);
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    if resp.ok() {
        resp.json::<PublicStatusResponse>()
            .await
            .map_err(|e| e.to_string())
    } else {
        Err(format!("HTTP error: {}", resp.status()))
    }
}

pub async fn get_public_status_by_subdomain(
    subdomain: &str,
) -> Result<PublicStatusResponse, String> {
    let url = format!("{}/status/{}", API_BASE, subdomain);
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    if resp.ok() {
        resp.json::<PublicStatusResponse>()
            .await
            .map_err(|e| e.to_string())
    } else {
        Err(format!("HTTP error: {}", resp.status()))
    }
}
