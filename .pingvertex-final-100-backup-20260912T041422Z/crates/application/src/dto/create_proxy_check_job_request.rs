use crate::dto::ProxyEntryInput;

/// Input data required to create a proxy-check job.
#[derive(Debug, Clone)]
pub struct CreateProxyCheckJobRequest {
    pub entries: Vec<ProxyEntryInput>,
}
