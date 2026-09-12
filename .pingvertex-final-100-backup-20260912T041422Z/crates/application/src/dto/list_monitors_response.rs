use domain::Monitor;

/// Response returned after listing monitors.
#[derive(Debug, Clone)]
pub struct ListMonitorsResponse {
    pub monitors: Vec<Monitor>,
}
