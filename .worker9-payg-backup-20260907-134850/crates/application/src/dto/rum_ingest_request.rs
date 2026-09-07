use serde::{Deserialize, Serialize};

/// Wire format for one browser beacon sent by the RUM SDK (Task D).
///
/// Deliberately a flat, permissive, all-optional-except-identifiers shape:
/// the SDK may send just a page view, just errors, or both in the same
/// beacon, and any individual metric may be missing depending on browser
/// support. Structural/security validation (non-empty, bounded lengths,
/// no query strings, bounded array sizes) happens in
/// `IngestRumEventUseCase`, NOT here — this type only describes what JSON
/// shape is accepted over the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RumIngestRequest {
    /// Public, non-secret site identifier (`RumSite::site_key`). Identifies
    /// which site this beacon belongs to; NOT a credential.
    pub site_key: String,

    /// Opaque, client-generated identifier grouping events from the same
    /// browser tab-session. Must not be (or contain) any PII.
    pub session_key: String,

    pub browser_name: Option<String>,
    pub browser_version: Option<String>,
    pub os_name: Option<String>,
    /// One of "desktop" | "mobile" | "tablet" | "other". Anything else is
    /// rejected by the use case rather than guessed at.
    pub device_type: Option<String>,

    pub page_view: Option<RumIngestPageView>,

    #[serde(default)]
    pub js_errors: Vec<RumIngestJsError>,

    #[serde(default)]
    pub resource_errors: Vec<RumIngestResourceError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RumIngestPageView {
    /// Path only — no scheme/host/query string/fragment. The SDK is
    /// responsible for stripping those before sending (see Task D); this
    /// is re-checked, not re-derived, by the domain layer.
    pub path: String,
    pub referrer_path: Option<String>,
    pub ttfb_ms: Option<i32>,
    pub fcp_ms: Option<i32>,
    pub lcp_ms: Option<i32>,
    pub inp_ms: Option<i32>,
    pub cls: Option<f64>,
    pub load_time_ms: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RumIngestJsError {
    pub message: String,
    pub source: Option<String>,
    pub line_number: Option<i32>,
    pub column_number: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RumIngestResourceError {
    pub resource_url: String,
    /// One of "script" | "stylesheet" | "image" | "fetch" | "other".
    pub resource_type: Option<String>,
    pub status_code: Option<i32>,
}
