#![allow(dead_code)]
use js_sys::encode_uri_component;
use serde::{Deserialize, Serialize};

use crate::api::client::{self, ApiError};

fn to_string_err(e: ApiError) -> String {
    e.to_string()
}

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

// ---------- Legacy compatibility structs (for pages) ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LegacyRumOverview {
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
pub struct LegacyRumTrendPoint {
    pub timestamp: String,
    pub page_views: u64,
    pub avg_lcp: f64,
    pub avg_cls: f64,
    pub avg_fcp: f64,
    pub error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LegacyRumPagePerformance {
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
pub struct LegacyRumSlowPage {
    pub path: String,
    pub view_count: u64,
    pub avg_lcp: f64,
    pub p75_lcp: f64,
    pub p95_lcp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LegacyRumJsError {
    pub message: String,
    pub stack: Option<String>,
    pub error_count: u64,
    pub affected_sessions: u64,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LegacyRumBrowserStat {
    pub browser: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LegacyRumDeviceStat {
    pub device_type: String,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LegacyRumSession {
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
    pub overview: LegacyRumOverview,
    pub trends: Vec<LegacyRumTrendPoint>,
    pub pages: Vec<LegacyRumPagePerformance>,
    pub slow_pages: Vec<LegacyRumSlowPage>,
    pub js_errors: Vec<LegacyRumJsError>,
    pub browser_stats: Vec<LegacyRumBrowserStat>,
    pub device_stats: Vec<LegacyRumDeviceStat>,
    pub sessions: Vec<LegacyRumSession>,
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

// ---------- Legacy compatibility functions (for pages, return String) ----------

pub async fn fetch_rum_sites() -> Result<Vec<RumSite>, String> {
    list_rum_sites_typed().await.map_err(to_string_err)
}

pub async fn fetch_rum_dashboard(
    site_id: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: Option<u32>,
) -> Result<RumDashboardData, String> {
    if site_id.is_none() {
        return Ok(RumDashboardData::default());
    }
    let sid = site_id.unwrap();
    let limit_i64 = limit.map(|l| l as i64);

    let overview = rum_overview_typed(sid, from, to)
        .await
        .map_err(to_string_err)?;
    let pages = rum_slow_pages_typed(sid, from, to, limit_i64)
        .await
        .map_err(to_string_err)?;
    let errors = rum_error_breakdown_typed(sid, from, to, limit_i64)
        .await
        .map_err(to_string_err)?;
    let browsers_devices = rum_browser_device_breakdown_typed(sid, from, to)
        .await
        .map_err(to_string_err)?;
    let trend = rum_performance_trend_typed(sid, from, to)
        .await
        .map_err(to_string_err)?;

    let legacy_overview = LegacyRumOverview {
        total_page_views: overview.page_view_count as u64,
        total_sessions: overview.session_count as u64,
        avg_lcp: overview.p75_lcp_ms.unwrap_or(0.0),
        avg_cls: overview.p75_cls.unwrap_or(0.0),
        avg_fcp: 0.0,
        avg_ttfb: 0.0,
        avg_fid: 0.0,
        avg_inp: overview.p75_inp_ms.unwrap_or(0.0),
        error_rate: overview.error_rate.unwrap_or(0.0),
    };

    let legacy_pages: Vec<LegacyRumPagePerformance> = pages
        .iter()
        .map(|p| LegacyRumPagePerformance {
            path: p.path.clone(),
            view_count: p.view_count as u64,
            avg_lcp: p.avg_lcp_ms.unwrap_or(0.0),
            avg_cls: 0.0,
            avg_fcp: 0.0,
            avg_ttfb: p.avg_load_time_ms.unwrap_or(0.0),
            error_count: 0,
        })
        .collect();

    let legacy_slow_pages: Vec<LegacyRumSlowPage> = pages
        .iter()
        .map(|p| LegacyRumSlowPage {
            path: p.path.clone(),
            view_count: p.view_count as u64,
            avg_lcp: p.avg_lcp_ms.unwrap_or(0.0),
            p75_lcp: p.avg_lcp_ms.unwrap_or(0.0),
            p95_lcp: p.p95_load_time_ms.unwrap_or(0.0),
        })
        .collect();

    let legacy_js_errors: Vec<LegacyRumJsError> = errors
        .top_js_errors
        .iter()
        .map(|e| LegacyRumJsError {
            message: e.message.clone(),
            stack: None,
            error_count: e.occurrence_count as u64,
            affected_sessions: 0,
            last_seen: String::new(),
        })
        .collect();

    let legacy_browser_stats: Vec<LegacyRumBrowserStat> = browsers_devices
        .browsers
        .iter()
        .map(|b| {
            let total_sessions = browsers_devices
                .browsers
                .iter()
                .map(|x| x.session_count)
                .sum::<i64>()
                .max(1) as f64;
            LegacyRumBrowserStat {
                browser: b
                    .browser_name
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
                count: b.session_count as u64,
                percentage: (b.session_count as f64 / total_sessions) * 100.0,
            }
        })
        .collect();

    let legacy_device_stats: Vec<LegacyRumDeviceStat> = browsers_devices
        .devices
        .iter()
        .map(|d| {
            let total_sessions = browsers_devices
                .devices
                .iter()
                .map(|x| x.session_count)
                .sum::<i64>()
                .max(1) as f64;
            LegacyRumDeviceStat {
                device_type: d
                    .device_type
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
                count: d.session_count as u64,
                percentage: (d.session_count as f64 / total_sessions) * 100.0,
            }
        })
        .collect();

    let legacy_trend: Vec<LegacyRumTrendPoint> = trend
        .iter()
        .map(|t| LegacyRumTrendPoint {
            timestamp: t.bucket_start.clone(),
            page_views: t.page_view_count as u64,
            avg_lcp: t.avg_lcp_ms.unwrap_or(0.0),
            avg_cls: 0.0,
            avg_fcp: 0.0,
            error_count: 0,
        })
        .collect();

    Ok(RumDashboardData {
        overview: legacy_overview,
        trends: legacy_trend,
        pages: legacy_pages,
        slow_pages: legacy_slow_pages,
        js_errors: legacy_js_errors,
        browser_stats: legacy_browser_stats,
        device_stats: legacy_device_stats,
        sessions: Vec::new(),
    })
}
