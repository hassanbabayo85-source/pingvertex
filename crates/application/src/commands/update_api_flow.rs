use domain::shared::ids::{ApiFlowId, MonitorId, UserId};

use crate::dto::ApiFlowStepInput;

#[derive(Debug, Clone)]
pub struct UpdateApiFlow {
    pub id: ApiFlowId,
    pub monitor_id: MonitorId,
    pub user_id: UserId,
    pub steps: Vec<ApiFlowStepInput>,
}
