use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::ApiFlowStep;
use crate::errors::DomainError;
use crate::shared::ids::{ApiFlowId, MonitorId, TenantId};
use crate::shared::policies::TenantOwned;

/// Hard cap on the number of steps a single API flow may contain.
///
/// `steps` is a client-supplied, unbounded `Vec<ApiFlowStep>` with no prior
/// limit — a flow with hundreds of steps would run for an unbounded amount
/// of time on the worker, tying up execution resources for no legitimate
/// reason (a real flow needs only a handful of sequential requests). 10
/// comfortably covers real use while blocking abuse.
pub const MAX_STEPS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiFlow {
    pub id: ApiFlowId,
    pub monitor_id: MonitorId,
    pub tenant_id: TenantId,
    pub steps: Vec<ApiFlowStep>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApiFlow {
    pub fn new(
        id: ApiFlowId,
        monitor_id: MonitorId,
        tenant_id: TenantId,
        steps: Vec<ApiFlowStep>,
    ) -> Result<Self, DomainError> {
        if steps.is_empty() {
            return Err(DomainError::EmptyFlowSteps);
        }

        if steps.len() > MAX_STEPS {
            return Err(DomainError::TooManyFlowSteps);
        }

        let mut orders: Vec<u16> = steps.iter().map(|step| step.order).collect();
        orders.sort_unstable();

        let expected: Vec<u16> = (1..=steps.len() as u16).collect();

        if orders != expected {
            return Err(DomainError::InvalidStepOrder);
        }

        let now = Utc::now();

        Ok(Self {
            id,
            monitor_id,
            tenant_id,
            steps,
            created_at: now,
            updated_at: now,
        })
    }

    /// Returns all steps as a read-only slice.
    pub fn steps(&self) -> &[ApiFlowStep] {
        &self.steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::HttpMethod;
    use crate::shared::ids::MonitorId;
    use crate::value_objects::FlowStepName;

    fn step(order: u16) -> ApiFlowStep {
        ApiFlowStep::new(
            order,
            FlowStepName::new(format!("Step {order}")).unwrap(),
            HttpMethod::Get,
            "https://example.com".to_string(),
            vec![],
            None,
            vec![],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn accepts_valid_flow() {
        let flow = ApiFlow::new(
            ApiFlowId::new(),
            MonitorId::new(),
            TenantId::new(),
            vec![step(1), step(2), step(3)],
        )
        .unwrap();

        assert_eq!(flow.steps().len(), 3);
    }

    #[test]
    fn rejects_empty_steps() {
        let result = ApiFlow::new(ApiFlowId::new(), MonitorId::new(), TenantId::new(), vec![]);

        assert_eq!(result, Err(DomainError::EmptyFlowSteps));
    }

    #[test]
    fn rejects_too_many_steps() {
        let steps = (1..=(MAX_STEPS as u16 + 1)).map(step).collect();

        let result = ApiFlow::new(ApiFlowId::new(), MonitorId::new(), TenantId::new(), steps);

        assert_eq!(result, Err(DomainError::TooManyFlowSteps));
    }

    #[test]
    fn rejects_duplicate_order() {
        let result = ApiFlow::new(
            ApiFlowId::new(),
            MonitorId::new(),
            TenantId::new(),
            vec![step(1), step(1)],
        );

        assert_eq!(result, Err(DomainError::InvalidStepOrder));
    }

    #[test]
    fn rejects_gap_in_order() {
        let result = ApiFlow::new(
            ApiFlowId::new(),
            MonitorId::new(),
            TenantId::new(),
            vec![step(1), step(3)],
        );

        assert_eq!(result, Err(DomainError::InvalidStepOrder));
    }

    #[test]
    fn rejects_order_not_starting_at_one() {
        let result = ApiFlow::new(
            ApiFlowId::new(),
            MonitorId::new(),
            TenantId::new(),
            vec![step(2), step(3)],
        );

        assert_eq!(result, Err(DomainError::InvalidStepOrder));
    }
}

impl TenantOwned for ApiFlow {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}
