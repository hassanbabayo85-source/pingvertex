use domain::{MonitorId, StatusPageId};

/// Command for updating a status page's name, monitor selection, logo, and
/// brand color. Does NOT touch the custom domain — that has its own
/// dedicated command/use case (`RequestCustomDomain`) since it triggers a
/// separate out-of-band DNS verification flow.
#[derive(Debug, Clone)]
pub struct UpdateStatusPage {
    pub id: StatusPageId,
    pub name: String,
    pub monitor_ids: Vec<MonitorId>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
}
