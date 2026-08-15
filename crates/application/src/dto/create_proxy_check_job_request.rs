use crate::dto::ProxyEntryInput;

/// Input data required to create a proxy-check job.
#[derive(Debug, Clone)]
pub struct CreateProxyCheckJobRequest {
    pub user_id: domain::shared::ids::UserId,
    pub entries: Vec<ProxyEntryInput>,
}
