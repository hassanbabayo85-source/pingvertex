use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::shared::policies::OwnershipPolicy;
use domain::shared::types::AuthenticatedUser;
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, EscalationPolicy};
use serde_json::json;

use crate::{
    commands::CreateEscalationPolicy,
    ports::{
        AuditLogEntryRepository, EscalationPolicyRepository, IdGenerator, MonitorRepository,
        NotificationChannelRepository,
    },
};

/// Error returned by `CreateEscalationPolicyUseCase`.
#[derive(Debug)]
pub enum CreateEscalationPolicyError {
    MonitorNotFound,
    ChannelNotFound,
    InvalidSteps(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateEscalationPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MonitorNotFound => write!(f, "monitor not found or access denied"),
            Self::ChannelNotFound => write!(f, "notification channel not found or access denied"),
            Self::InvalidSteps(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateEscalationPolicyError {}

pub struct CreateEscalationPolicyUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
    id_generator: Arc<dyn IdGenerator>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl CreateEscalationPolicyUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
        id_generator: Arc<dyn IdGenerator>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            notification_channel_repository,
            escalation_policy_repository,
            id_generator,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: CreateEscalationPolicy,
        authenticated_user: AuthenticatedUser,
    ) -> Result<EscalationPolicy, CreateEscalationPolicyError> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        if let Some(monitor_id) = command.monitor_id {
            let monitor = self
                .monitor_repository
                .find_by_id_for_user(monitor_id, user_id)
                .await
                .map_err(CreateEscalationPolicyError::Other)?
                .ok_or(CreateEscalationPolicyError::MonitorNotFound)?;

            if !OwnershipPolicy::can_access(&authenticated_user, &monitor) {
                return Err(CreateEscalationPolicyError::MonitorNotFound);
            }
        }

        for step in &command.steps {
            let channel = self
                .notification_channel_repository
                .find_by_id_for_user(step.channel_id, user_id)
                .await
                .map_err(CreateEscalationPolicyError::Other)?
                .ok_or(CreateEscalationPolicyError::ChannelNotFound)?;

            if !OwnershipPolicy::can_access(&authenticated_user, &channel) {
                return Err(CreateEscalationPolicyError::ChannelNotFound);
            }

            if let Some(monitor_id) = command.monitor_id {
                if channel.monitor_id != monitor_id {
                    return Err(CreateEscalationPolicyError::ChannelNotFound);
                }
            }
        }

        let id = self.id_generator.next_escalation_policy_id().await;

        let policy =
            EscalationPolicy::new(id, user_id, tenant_id, command.monitor_id, command.steps)
                .map_err(|err| CreateEscalationPolicyError::InvalidSteps(err.to_string()))?;

        self.escalation_policy_repository
            .create(&policy)
            .await
            .map_err(CreateEscalationPolicyError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::EscalationPolicyCreated,
            target_id: policy.id.0.to_string(),
            metadata: json!({
                "has_monitor": policy.monitor_id.is_some(),
                "steps": policy.steps.len(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(CreateEscalationPolicyError::Other)?;

        Ok(policy)
    }
}
