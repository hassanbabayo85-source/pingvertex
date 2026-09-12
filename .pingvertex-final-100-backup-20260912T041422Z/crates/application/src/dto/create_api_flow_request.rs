use crate::dto::ApiFlowStepInput;

/// Input data required to create an API flow.
#[derive(Debug, Clone)]
pub struct CreateApiFlowRequest {
    pub monitor_id: domain::shared::ids::MonitorId,
    pub steps: Vec<ApiFlowStepInput>,
}
