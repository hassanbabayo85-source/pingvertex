use domain::Monitor;

/// Response returned after retrieving a monitor.
#[derive(Debug, Clone)]
pub struct GetMonitorResponse {
    pub monitor: Monitor,
}
