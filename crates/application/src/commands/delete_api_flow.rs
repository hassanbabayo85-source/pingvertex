use domain::shared::ids::{ApiFlowId, MonitorId};

#[derive(Debug, Clone, Copy)]
pub struct DeleteApiFlow {
    pub id: ApiFlowId,
    pub monitor_id: MonitorId,
}
