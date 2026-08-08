use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{CheckId, MonitorId};

use crate::enums::{CheckStatus, ErrorType};

/// Represents one execution of a monitor check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub id: CheckId,
    pub monitor_id: MonitorId,

    pub status: CheckStatus,
    pub status_code: Option<u16>,

    pub latency_ms: u128,

    pub error_type: ErrorType,
    pub error_message: Option<String>,

    pub checked_at: DateTime<Utc>,
}

impl Check {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: CheckId,
        monitor_id: MonitorId,
        status: CheckStatus,
        status_code: Option<u16>,
        latency_ms: u128,
        error_type: ErrorType,
        error_message: Option<String>,
    ) -> Self {
        Self {
            id,
            monitor_id,
            status,
            status_code,
            latency_ms,
            error_type,
            error_message,
            checked_at: Utc::now(),
        }
    }
}
