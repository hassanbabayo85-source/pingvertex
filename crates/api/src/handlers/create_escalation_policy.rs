use std::sync::Arc;

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use tracing::info;

use application::{
    commands::CreateEscalationPolicy,
    use_cases::{CreateEscalationPolicyError, CreateEscalationPolicyUseCase},
};
use domain::{shared::ids::UserId, EscalationStep};

use crate::{
    error::ApiError, handlers::feature_types::CreateEscalationPolicyRequest, state::AppState,
    validation::parse_notification_channel_id,
};

/// Creates an escalation policy.
///
/// `POST /api/v1/escalation-policies`
///
/// The request may include `monitor_id` to create a monitor-specific
/// policy, or omit it to create a global policy for the user. All
/// channels referenced in the steps must exist and belong to the user
/// (and, for monitor-specific policies, to that monitor).
pub async fn create_escalation_policy(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Json(payload): Json<CreateEscalationPolicyRequest>,
) -> Result<(StatusCode, Json<domain::EscalationPolicy>), ApiError> {
    info!("API: create_escalation_policy");

    // Parse optional monitor id
    let monitor_id = match payload.monitor_id {
        Some(raw) => Some(crate::validation::parse_monitor_id(&raw)?),
        None => None,
    };

    // Parse steps
    let mut steps = Vec::with_capacity(payload.steps.len());
    for step_input in payload.steps {
        let channel_id = parse_notification_channel_id(&step_input.channel_id)?;
        steps.push(EscalationStep {
            delay_minutes: step_input.delay_minutes,
            channel_id,
        });
    }

    let use_case = CreateEscalationPolicyUseCase::new(
        state.monitor_repository.clone(),
        state.notification_channel_repository.clone(),
        state.escalation_policy_repository.clone(),
        Arc::new(state.id_generator.clone()),
    );

    let policy = use_case
        .execute(CreateEscalationPolicy {
            user_id,
            monitor_id,
            steps,
        })
        .await
        .map_err(|err| match err {
            CreateEscalationPolicyError::MonitorNotFound => {
                ApiError::not_found("monitor not found")
            }
            CreateEscalationPolicyError::ChannelNotFound => {
                ApiError::not_found("notification channel not found")
            }
            CreateEscalationPolicyError::InvalidSteps(msg) => ApiError::bad_request(msg),
            CreateEscalationPolicyError::Other(err) => {
                ApiError::internal_with_source("failed to create escalation policy", err)
            }
        })?;

    Ok((StatusCode::CREATED, Json(policy)))
}
