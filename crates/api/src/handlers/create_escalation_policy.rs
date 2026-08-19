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
use domain::{shared::types::AuthenticatedUser, EscalationStep};

use crate::{
    error::ApiError, handlers::feature_types::CreateEscalationPolicyRequest, state::AppState,
    validation::parse_notification_channel_id,
};

pub async fn create_escalation_policy(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateEscalationPolicyRequest>,
) -> Result<(StatusCode, Json<domain::EscalationPolicy>), ApiError> {
    info!("API: create_escalation_policy");

    let monitor_id = match payload.monitor_id {
        Some(raw) => Some(crate::validation::parse_monitor_id(&raw)?),
        None => None,
    };

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
        state.audit_log_entry_repository.clone(),
    );

    let policy = use_case
        .execute(
            CreateEscalationPolicy { monitor_id, steps },
            authenticated_user,
        )
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
