use std::{error::Error, fmt, sync::Arc};

use domain::EscalationPolicy;

use crate::{
    commands::CreateEscalationPolicy,
    ports::{
        EscalationPolicyRepository, IdGenerator, MonitorRepository, NotificationChannelRepository,
    },
};

/// Error returned by `CreateEscalationPolicyUseCase`.
#[derive(Debug)]
pub enum CreateEscalationPolicyError {
    /// Monitor doesn't exist, or exists but isn't owned by this user.
    MonitorNotFound,
    /// A notification channel in the steps doesn't exist, isn't owned by
    /// this user, or (for monitor-specific policies) doesn't belong to
    /// the specified monitor.
    ChannelNotFound,
    /// Escalation step validation failed.
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

/// Creates an escalation policy, verifying that:
/// - the monitor (if specified) belongs to the user;
/// - every channel in the steps belongs to the user and, for monitor-specific
///   policies, belongs to that exact monitor.
pub struct CreateEscalationPolicyUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
    id_generator: Arc<dyn IdGenerator>,
}

impl CreateEscalationPolicyUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        notification_channel_repository: Arc<dyn NotificationChannelRepository>,
        escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            monitor_repository,
            notification_channel_repository,
            escalation_policy_repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        command: CreateEscalationPolicy,
    ) -> Result<EscalationPolicy, CreateEscalationPolicyError> {
        // Idan monitor_id ya kasance Some, dole mu tabbatar mallaka.
        if let Some(monitor_id) = command.monitor_id {
            self.monitor_repository
                .find_by_id_for_user(monitor_id, command.user_id)
                .await
                .map_err(CreateEscalationPolicyError::Other)?
                .ok_or(CreateEscalationPolicyError::MonitorNotFound)?;
        }

        // Tabbatar da kowanne channel a cikin steps.
        for step in &command.steps {
            let channel = self
                .notification_channel_repository
                .find_by_id_for_user(step.channel_id, command.user_id)
                .await
                .map_err(CreateEscalationPolicyError::Other)?
                .ok_or(CreateEscalationPolicyError::ChannelNotFound)?;

            // Idan policy ɗin monitor-specific ne, channel dole ya kasance
            // na wannan monitor.
            if let Some(monitor_id) = command.monitor_id {
                if channel.monitor_id != monitor_id {
                    return Err(CreateEscalationPolicyError::ChannelNotFound);
                }
            }
        }

        let id = self.id_generator.next_escalation_policy_id().await;

        let policy = EscalationPolicy::new(id, command.user_id, command.monitor_id, command.steps)
            .map_err(|err| CreateEscalationPolicyError::InvalidSteps(err.to_string()))?;

        self.escalation_policy_repository
            .create(&policy)
            .await
            .map_err(CreateEscalationPolicyError::Other)?;

        Ok(policy)
    }
}
