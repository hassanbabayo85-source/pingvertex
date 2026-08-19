use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    errors::DomainError,
    shared::{
        ids::{EscalationPolicyId, MonitorId, NotificationChannelId, TenantId, UserId},
        policies::{Ownable, TenantOwned},
    },
};

pub const MAX_ESCALATION_STEPS: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EscalationStep {
    pub delay_minutes: u32,
    pub channel_id: NotificationChannelId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EscalationPolicy {
    pub id: EscalationPolicyId,
    pub user_id: UserId,
    pub tenant_id: TenantId,
    pub monitor_id: Option<MonitorId>,
    pub steps: Vec<EscalationStep>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EscalationPolicy {
    pub fn new(
        id: EscalationPolicyId,
        user_id: UserId,
        tenant_id: TenantId,
        monitor_id: Option<MonitorId>,
        steps: Vec<EscalationStep>,
    ) -> Result<Self, DomainError> {
        Self::validate_steps(&steps)?;
        let now = Utc::now();
        Ok(Self {
            id,
            user_id,
            tenant_id,
            monitor_id,
            steps,
            created_at: now,
            updated_at: now,
        })
    }

    fn validate_steps(steps: &[EscalationStep]) -> Result<(), DomainError> {
        if steps.is_empty() {
            return Err(DomainError::InvalidEscalationSteps(
                "escalation policy must have at least one step".to_string(),
            ));
        }
        if steps.len() > MAX_ESCALATION_STEPS {
            return Err(DomainError::InvalidEscalationSteps(format!(
                "escalation policy cannot have more than {MAX_ESCALATION_STEPS} steps"
            )));
        }
        Ok(())
    }
}

impl Ownable for EscalationPolicy {
    fn owner_id(&self) -> UserId {
        self.user_id
    }
}

impl TenantOwned for EscalationPolicy {
    fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }
}
