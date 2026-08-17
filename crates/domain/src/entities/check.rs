use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{CheckId, MonitorId};

use crate::enums::{CheckStatus, ErrorType};
use crate::value_objects::RegionCode;

/// Represents one execution of a monitor check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub id: CheckId,
    pub monitor_id: MonitorId,

    pub region: RegionCode,

    pub status: CheckStatus,
    pub status_code: Option<u16>,

    pub latency_ms: u128,

    pub error_type: ErrorType,
    pub error_message: Option<String>,

    pub checked_at: DateTime<Utc>,

    pub cert_expires_at: Option<DateTime<Utc>>,
    pub cert_days_remaining: Option<i64>,
}

impl Check {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: CheckId,
        monitor_id: MonitorId,
        region: RegionCode,
        status: CheckStatus,
        status_code: Option<u16>,
        latency_ms: u128,
        error_type: ErrorType,
        error_message: Option<String>,
    ) -> Self {
        Self {
            id,
            monitor_id,
            region,
            status,
            status_code,
            latency_ms,
            error_type,
            error_message,
            checked_at: Utc::now(),
            cert_expires_at: None,
            cert_days_remaining: None,
        }
    }

    /// Attaches TLS certificate expiry info to this check. Consuming
    /// builder-style method so call sites that don't check certs (e.g.
    /// non-HTTPS monitors) can leave these fields as `None`.
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
