/// Result of a single monitor check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub success: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u128,
    pub error: Option<String>,
}

impl CheckResult {
    pub fn success(status_code: u16, latency_ms: u128) -> Self {
        Self {
            success: true,
            status_code: Some(status_code),
            latency_ms,
            error: None,
        }
    }

    pub fn failure(latency_ms: u128, error: impl Into<String>) -> Self {
        Self {
            success: false,
            status_code: None,
            latency_ms,
            error: Some(error.into()),
        }
    }
}
