use domain::{MonitorId, UserId};

/// Client-supplied input for creating a status page.
///
/// `subdomain` is the raw string as submitted — validated into a
/// `domain::Subdomain` value object inside the use case, not here, so the
/// DTO stays a plain data holder with no domain-validation logic.
#[derive(Debug, Clone)]
pub struct CreateStatusPageRequest {
    pub user_id: UserId,
    pub name: String,
    pub subdomain: String,
    pub monitor_ids: Vec<MonitorId>,
}
