#![allow(dead_code)]
use gloo_net::http::Request;
use js_sys::encode_uri_component;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use web_sys::RequestCredentials;

const API_BASE: &str = "/api";

fn build_url_with_query(path: &str, query: &[(&str, &str)]) -> String {
    if query.is_empty() {
        return format!("{}{}", API_BASE, path);
    }
    let mut parts = Vec::new();
    for (k, v) in query {
        let key = encode_uri_component(k).as_string().unwrap_or_default();
        let value = encode_uri_component(v).as_string().unwrap_or_default();
        parts.push(format!("{}={}", key, value));
    }
    format!("{}{}?{}", API_BASE, path, parts.join("&"))
}

async fn get_json_with_query(path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
    let url = build_url_with_query(path, query);
    let response = Request::get(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(|e| format!("Invalid JSON response: {e}"))
    } else {
        Err(format!("Request failed with status {status}: {text}"))
    }
}

// ---------- Models ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumSite {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub public_site_key: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumOverview {
    pub total_page_views: u64,
    pub total_sessions: u64,
    pub avg_lcp: f64,
    pub avg_cls: f64,
    pub avg_fcp: f64,
    pub avg_ttfb: f64,
    pub avg_fid: f64,
    pub avg_inp: f64,
    pub error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumTrendPoint {
    pub timestamp: String,
    pub page_views: u64,
    pub avg_lcp: f64,
    pub avg_cls: f64,
    pub avg_fcp: f64,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumPagePerformance {
    pub path: String,
    pub view_count: u64,
    pub avg_lcp: f64,
    pub avg_cls: f64,
    pub avg_fcp: f64,
    pub avg_ttfb: f64,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumSlowPage {
    pub path: String,
    pub view_count: u64,
    pub avg_lcp: f64,
    pub p75_lcp: f64,
    pub p95_lcp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumJsError {
    pub message: String,
    pub stack: Option<String>,
    pub error_count: u64,
    pub affected_sessions: u64,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumBrowserStat {
    pub browser: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumDeviceStat {
    pub device_type: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumSession {
    pub session_id: String,
    pub country_code: String,
    pub device_type: String,
    pub browser: String,
    pub os: String,
    pub started_at: String,
    pub last_seen_at: String,
    pub page_views: u64,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RumDashboardData {
    pub overview: RumOverview,
    pub trends: Vec<RumTrendPoint>,
    pub pages: Vec<RumPagePerformance>,
    pub slow_pages: Vec<RumSlowPage>,
    pub js_errors: Vec<RumJsError>,
    pub browser_stats: Vec<RumBrowserStat>,
    pub device_stats: Vec<RumDeviceStat>,
    pub sessions: Vec<RumSession>,
}

// ---------- API functions ----------

pub async fn fetch_rum_sites() -> Result<Vec<RumSite>, String> {
    let value = get_json_with_query("/rum/sites", &[]).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid RUM sites data: {e}"))
}

pub async fn fetch_rum_dashboard(
    site_id: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<u32>,
) -> Result<RumDashboardData, String> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(s) = site_id {
        query.push(("site_id", s));
    }
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str = limit.map(|l| l.to_string());
    if let Some(ref l) = limit_str {
        query.push(("limit", l.as_str()));
    }
    let value = get_json_with_query("/rum/dashboard", &query).await?;
    serde_json::from_value(value).map_err(|e| format!("Invalid RUM dashboard data: {e}"))
}
