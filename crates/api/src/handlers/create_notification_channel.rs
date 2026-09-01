use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use std::sync::Arc;
use tracing::info;

use application::{
    commands::CreateNotificationChannel,
    use_cases::{CreateNotificationChannelError, CreateNotificationChannelUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::types::{CreateNotificationChannelRequest, NotificationChannelCreateResponse},
    state::AppState,
    validation::parse_monitor_id,
};

pub async fn create_notification_channel(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(monitor_id): Path<String>,
    Json(payload): Json<CreateNotificationChannelRequest>,
) -> Result<(StatusCode, Json<NotificationChannelCreateResponse>), ApiError> {
    info!("API: create_notification_channel");

    let monitor_id = parse_monitor_id(&monitor_id)?;

    let use_case = CreateNotificationChannelUseCase::new(
        state.monitor_repository.clone(),
        state.notification_channel_repository.clone(),
        Arc::new(state.id_generator.clone()),
        state.secret_cipher.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let channel = use_case
        .execute(
            CreateNotificationChannel {
                monitor_id,
                kind: payload.kind,
                target: payload.target,
                chat_id: payload.chat_id,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| match err {
            CreateNotificationChannelError::Forbidden => {
                ApiError::forbidden("you do not have permission to create notification channels")
            }
            CreateNotificationChannelError::MonitorNotFound => {
                ApiError::not_found("monitor not found")
            }
            CreateNotificationChannelError::InvalidTarget(message) => {
                ApiError::bad_request(message)
            }
            CreateNotificationChannelError::Other(err) => {
                ApiError::internal_with_source("failed to create notification channel", err)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(NotificationChannelCreateResponse::from_created(channel)),
    ))
}
