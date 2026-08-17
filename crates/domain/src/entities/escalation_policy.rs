use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{EscalationPolicyId, MonitorId, NotificationChannelId, UserId};

/// Tsarin escalation: idan incident bai samu acknowledgment ba cikin
/// `delay_minutes` na kowane mataki, sai a tura notification zuwa
/// `channel_id` na wannan matakin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationPolicy {
    pub id: EscalationPolicyId,
    /// Wanda ya mallaki wannan policy.
    pub user_id: UserId,
    /// Idan `Some(monitor_id)` ne, policy ɗin yana aiki ne kawai ga wannan
    /// monitor. Idan `None` ne, yana aiki ga dukkan monitors na wannan
    /// user.
    pub monitor_id: Option<MonitorId>,
    pub steps: Vec<EscalationStep>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscalationStep {
    pub delay_minutes: u32,
    pub channel_id: NotificationChannelId,
}

impl EscalationPolicy {
    pub const MAX_STEPS: usize = 5;

    pub fn new(
        id: EscalationPolicyId,
        user_id: UserId,
        monitor_id: Option<MonitorId>,
        steps: Vec<EscalationStep>,
    ) -> Result<Self, DomainError> {
        if steps.is_empty() {
            return Err(DomainError::InvalidEscalationSteps(
                "steps cannot be empty".to_string(),
            ));
        }
        if steps.len() > Self::MAX_STEPS {
            return Err(DomainError::InvalidEscalationSteps(format!(
                "steps cannot exceed {}",
                Self::MAX_STEPS
            )));
        }
        let now = Utc::now();
        Ok(Self {
            id,
            user_id,
            monitor_id,
            steps,
            created_at: now,
            updated_at: now,
        })
    }
}
