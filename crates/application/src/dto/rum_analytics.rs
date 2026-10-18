use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Query-time window shared by every analytics method below. `start` is
/// inclusive, `end` is exclusive — callers (Task F's dashboard handlers)
/// are responsible for turning a user-facing "last 24h / 7d / 30d" picker
/// into concrete timestamps before calling into this layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RumTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Top-level KPIs for a site over a time range.
///
/// Percentile fields use p75 for Web Vitals (the threshold Google's own
/// Core Web Vitals scoring uses) and p95 for general load-time latency
/// (a more common "worst-case typical" latency percentile). All
/// percentile/average fields are `None` when there is no data with that
/// metric present in the range — NOT coerced to 0, since 0 would be a
/// real (and misleading) value for a timing metric.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RumOverview {
    pub page_view_count: i64,
    pub session_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub p95_load_time_ms: Option<f64>,
    pub p75_lcp_ms: Option<f64>,
    pub p75_inp_ms: Option<f64>,
    pub p75_cls: Option<f64>,
    pub js_error_count: i64,
    pub resource_error_count: i64,
    /// `(js_error_count + resource_error_count) / page_view_count`,
    /// `None` when `page_view_count` is 0 (avoids a division by zero
    /// masquerading as a real 0% error rate).
    pub error_rate: Option<f64>,
}

/// One row of the "page performance" / "slow pages" breakdown — grouped
/// by `path`, ordered by `avg_load_time_ms` descending (slowest first) by
/// `RumAnalyticsRepository::slow_pages`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PagePerformanceRow {
    pub path: String,
    pub view_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub p95_load_time_ms: Option<f64>,
    pub avg_lcp_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopJsErrorRow {
    pub message: String,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopResourceErrorRow {
    pub resource_url: String,
    pub occurrence_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumErrorBreakdown {
    pub js_error_count: i64,
    pub resource_error_count: i64,
    pub page_view_count: i64,
    pub error_rate: Option<f64>,
    pub top_js_errors: Vec<TopJsErrorRow>,
    pub top_resource_errors: Vec<TopResourceErrorRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserBreakdownRow {
    pub browser_name: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceBreakdownRow {
    pub device_type: Option<String>,
    pub session_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserDeviceBreakdown {
    pub browsers: Vec<BrowserBreakdownRow>,
    pub devices: Vec<DeviceBreakdownRow>,
}

/// One country's session count. `country_code` will only ever be
/// non-`None` for sessions ingested AFTER geo-IP lookup was wired in and
/// configured (`GEOIP_DB_PATH`). Before that (or if
/// geo-IP lookup fails for a given IP), sessions have `country_code =
/// NULL`, which surfaces here as a row with `country_code: None` rather
/// than being silently dropped, so the caller can see "unknown" as its
/// own bucket instead of the total undercounting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoBreakdownRow {
    pub country_code: Option<String>,
    pub session_count: i64,
}

/// One bucket of a performance-over-time chart. Buckets are fixed at
/// one-hour granularity (`date_trunc('hour', created_at)`) — see
/// the rationale for why configurable bucket width was deliberately not
/// implemented in this pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceTrendPoint {
    pub bucket_start: DateTime<Utc>,
    pub page_view_count: i64,
    pub avg_load_time_ms: Option<f64>,
    pub avg_lcp_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SessionStats {
    pub session_count: i64,
    pub avg_page_views_per_session: Option<f64>,
    pub avg_session_duration_secs: Option<f64>,
}
