use chrono::{DateTime, Utc};

use crate::entities::DiagnosticStep;
use crate::value_objects::RegionCode;

/// Result of a single monitor check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// The region from which this check was performed.
    pub region: RegionCode,
    pub success: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u128,
    pub error: Option<String>,
    pub cert_expires_at: Option<DateTime<Utc>>,
    pub cert_days_remaining: Option<i64>,
    /// Diagnostic steps captured immediately after a connectivity failure
    /// (DNS/TCP/TLS/HTTP), explaining WHY the check failed. Empty for a
    /// successful check, or for a failure where diagnostics were not run
    /// (e.g. an assertion failure — not a connectivity problem).
    pub diagnostic_steps: Vec<DiagnosticStep>,
    /// Template-generated root cause paired with `diagnostic_steps`.
    /// Always `None` when `diagnostic_steps` is empty.
    pub diagnostic_root_cause_summary: Option<String>,
}

impl CheckResult {
    pub fn success(region: RegionCode, status_code: u16, latency_ms: u128) -> Self {
        Self {
            region,
            success: true,
            status_code: Some(status_code),
            latency_ms,
            error: None,
            cert_expires_at: None,
            cert_days_remaining: None,
            diagnostic_steps: Vec::new(),
            diagnostic_root_cause_summary: None,
        }
    }

    pub fn failure(region: RegionCode, latency_ms: u128, error: impl Into<String>) -> Self {
        Self {
            region,
            success: false,
            status_code: None,
            latency_ms,
            error: Some(error.into()),
            cert_expires_at: None,
            cert_days_remaining: None,
            diagnostic_steps: Vec::new(),
            diagnostic_root_cause_summary: None,
        }
    }

    /// Creates a failure result that still carries the HTTP status code.
    /// Use this when the HTTP request succeeded but assertions failed.
    pub fn failure_with_status(
        region: RegionCode,
        status_code: u16,
        latency_ms: u128,
        error: impl Into<String>,
    ) -> Self {
        Self {
            region,
            success: false,
            status_code: Some(status_code),
            latency_ms,
            error: Some(error.into()),
            cert_expires_at: None,
            cert_days_remaining: None,
            diagnostic_steps: Vec::new(),
            diagnostic_root_cause_summary: None,
        }
    }

    /// Attaches TLS certificate expiry info to this result. Consuming
    /// builder-style method — call sites that don't check certs simply
    /// never call this, leaving both fields `None`.
    pub fn with_cert_info(
        mut self,
        cert_expires_at: Option<DateTime<Utc>>,
        cert_days_remaining: Option<i64>,
    ) -> Self {
        self.cert_expires_at = cert_expires_at;
        self.cert_days_remaining = cert_days_remaining;
        self
    }

    /// Attaches a diagnostic run's steps + template-generated root cause
    /// summary to this (failed) result. Consuming builder-style method,
    /// matching `with_cert_info` above.
    pub fn with_diagnostics(
        mut self,
        steps: Vec<DiagnosticStep>,
        root_cause_summary: impl Into<String>,
    ) -> Self {
        self.diagnostic_steps = steps;
        self.diagnostic_root_cause_summary = Some(root_cause_summary.into());
        self
    }
}
