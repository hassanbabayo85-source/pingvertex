use chrono::{DateTime, Utc};

/// Result of a single monitor check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub success: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u128,
    pub error: Option<String>,
    pub cert_expires_at: Option<DateTime<Utc>>,
    pub cert_days_remaining: Option<i64>,
}

impl CheckResult {
    pub fn success(status_code: u16, latency_ms: u128) -> Self {
        Self {
            success: true,
            status_code: Some(status_code),
            latency_ms,
            error: None,
            cert_expires_at: None,
            cert_days_remaining: None,
        }
    }

    pub fn failure(latency_ms: u128, error: impl Into<String>) -> Self {
        Self {
            success: false,
            status_code: None,
            latency_ms,
            error: Some(error.into()),
            cert_expires_at: None,
            cert_days_remaining: None,
        }
    }

    /// Creates a failure result that still carries the HTTP status code.
    /// Use this when the HTTP request succeeded but assertions failed.
    pub fn failure_with_status(
        status_code: u16,
        latency_ms: u128,
        error: impl Into<String>,
    ) -> Self {
        Self {
            success: false,
            status_code: Some(status_code),
            latency_ms,
            error: Some(error.into()),
            cert_expires_at: None,
            cert_days_remaining: None,
        }
    }

    /// Attaches TLS certificate expiry info to this result. Consuming
    /// builder-style method, matching `Check::with_cert_info` — call sites
    /// that don't check certs (non-HTTPS monitors, or `ssl_check_enabled ==
    /// false`) simply never call this, leaving both fields `None`.
    pub fn with_cert_info(
        mut self,
        cert_expires_at: Option<DateTime<Utc>>,
        cert_days_remaining: Option<i64>,
    ) -> Self {
        self.cert_expires_at = cert_expires_at;
        self.cert_days_remaining = cert_days_remaining;
        self
    }
}
