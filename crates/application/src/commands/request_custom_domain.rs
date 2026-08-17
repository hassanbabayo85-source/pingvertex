use domain::{StatusPageId, UserId};

/// Command for attaching (or re-attaching) a custom domain to a status
/// page. `domain` is the raw string as submitted — validated into a
/// `domain::CustomDomain` value object inside the use case.
#[derive(Debug, Clone)]
pub struct RequestCustomDomain {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub domain: String,
}
