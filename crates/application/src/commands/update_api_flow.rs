use domain::shared::ids::{ApiFlowId, MonitorId};

use crate::dto::ApiFlowStepInput;

#[derive(Debug, Clone)]
pub struct UpdateApiFlow {
    pub id: ApiFlowId,
    pub monitor_id: MonitorId,
    pub steps: Vec<ApiFlowStepInput>,
}
