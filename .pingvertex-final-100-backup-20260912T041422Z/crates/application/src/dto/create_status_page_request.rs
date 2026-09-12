use domain::MonitorId;

/// Client-supplied input for creating a status page.
#[derive(Debug, Clone)]
pub struct CreateStatusPageRequest {
    pub name: String,
    pub subdomain: String,
    pub monitor_ids: Vec<MonitorId>,
}
