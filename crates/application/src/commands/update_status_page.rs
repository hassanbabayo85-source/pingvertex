use domain::{MonitorId, StatusPageId, UserId};

/// Command for updating a status page's name, monitor selection, logo, and
/// brand color. Does NOT touch the custom domain — that has its own
/// dedicated command/use case (`RequestCustomDomain`) since it triggers a
/// separate out-of-band DNS verification flow.
#[derive(Debug, Clone)]
pub struct UpdateStatusPage {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub name: String,
    pub monitor_ids: Vec<MonitorId>,
    /// `None` clears the logo. `Some(url)` sets it — `url` is expected to
    /// already be a CDN URL from the (not-yet-built) logo upload flow.
    pub logo_url: Option<String>,
    /// `None` clears the brand color. `Some(hex)` is a raw `"#RRGGBB"`
    /// string, validated into `domain::HexColor` inside the use case.
    pub primary_color: Option<String>,
}
