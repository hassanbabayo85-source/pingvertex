use js_sys::encode_uri_component;
use serde::{Deserialize, Serialize};

use crate::api::client::{self, ApiError};

fn build_url_with_query(path: &str, query: &[(&str, &str)]) -> String {
    if query.is_empty() {
        return path.to_string();
    }
    let mut parts = Vec::new();
    for (k, v) in query {
        let key = encode_uri_component(k).as_string().unwrap_or_default();
        let value = encode_uri_component(v).as_string().unwrap_or_default();
        parts.push(format!("{}={}", key, value));
    }
    format!("{}?{}", path, parts.join("&"))
}

// ---------- NEW DTOs (backend contract) ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RumSite {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub site_key: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CreateRumSitePayload {
    pub name: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendRumOverview {
    pub page_view_count: i64,
    pub session_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub p95_load_time_ms: Option<f64>,
    pub p75_lcp_ms: Option<f64>,
    pub p75_inp_ms: Option<f64>,
    pub p75_cls: Option<f64>,
    pub js_error_count: i64,
    pub resource_error_count: i64,
    pub error_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendPagePerformanceRow {
    pub path: String,
    pub view_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub p95_load_time_ms: Option<f64>,
    pub avg_lcp_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendTopJsErrorRow {
    pub message: String,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendTopResourceErrorRow {
    pub resource_url: String,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendRumErrorBreakdown {
    pub js_error_count: i64,
    pub resource_error_count: i64,
    pub page_view_count: i64,
    pub error_rate: Option<f64>,
    pub top_js_errors: Vec<BackendTopJsErrorRow>,
    pub top_resource_errors: Vec<BackendTopResourceErrorRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendBrowserBreakdownRow {
    pub browser_name: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendDeviceBreakdownRow {
    pub device_type: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendBrowserDeviceBreakdown {
    pub browsers: Vec<BackendBrowserBreakdownRow>,
    pub devices: Vec<BackendDeviceBreakdownRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendGeoBreakdownRow {
    pub country_code: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendPerformanceTrendPoint {
    pub bucket_start: String,
    pub page_view_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub avg_lcp_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BackendSessionStats {
    pub session_count: i64,
    pub avg_page_views_per_session: Option<f64>,
    pub avg_session_duration_secs: Option<f64>,
}

// ---------- New API functions (backend contract, typed) ----------

pub async fn list_rum_sites_typed() -> Result<Vec<RumSite>, ApiError> {
    let url = build_url_with_query("/dashboard/rum/sites", &[]);
    client::get_json::<Vec<RumSite>>(&url).await
}

pub async fn create_rum_site_typed(payload: &CreateRumSitePayload) -> Result<RumSite, ApiError> {
    client::post_json::<RumSite, _>("/dashboard/rum/sites", payload).await
}

pub async fn rum_overview_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BackendRumOverview, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/overview", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendRumOverview>(&url).await
}

pub async fn rum_slow_pages_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<BackendPagePerformanceRow>, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str;
    if let Some(l) = limit {
        limit_str = l.to_string();
        query.push(("limit", &limit_str));
    }
    let path = format!("/dashboard/rum/sites/{}/pages", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<Vec<BackendPagePerformanceRow>>(&url).await
}

pub async fn rum_error_breakdown_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<i64>,
) -> Result<BackendRumErrorBreakdown, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str;
    if let Some(l) = limit {
        limit_str = l.to_string();
        query.push(("limit", &limit_str));
    }
    let path = format!("/dashboard/rum/sites/{}/errors", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendRumErrorBreakdown>(&url).await
}

pub async fn rum_browser_device_breakdown_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BackendBrowserDeviceBreakdown, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/browsers-devices", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendBrowserDeviceBreakdown>(&url).await
}

pub async fn rum_geo_breakdown_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<BackendGeoBreakdownRow>, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let limit_str;
    if let Some(l) = limit {
        limit_str = l.to_string();
        query.push(("limit", &limit_str));
    }
    let path = format!("/dashboard/rum/sites/{}/geo", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<Vec<BackendGeoBreakdownRow>>(&url).await
}

pub async fn rum_performance_trend_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Vec<BackendPerformanceTrendPoint>, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/trend", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<Vec<BackendPerformanceTrendPoint>>(&url).await
}

pub async fn rum_session_stats_typed(
    site_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BackendSessionStats, ApiError> {
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(f) = from {
        query.push(("from", f));
    }
    if let Some(t) = to {
        query.push(("to", t));
    }
    let path = format!("/dashboard/rum/sites/{}/sessions", site_id);
    let url = build_url_with_query(&path, &query);
    client::get_json::<BackendSessionStats>(&url).await
}
