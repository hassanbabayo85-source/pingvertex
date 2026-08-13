use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::info;

use application::{
    commands::CreateNotificationChannel,
    use_cases::{CreateNotificationChannelError, CreateNotificationChannelUseCase},
};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError,
    handlers::types::{CreateNotificationChannelRequest, NotificationChannelCreateResponse},
    state::AppState,
    validation::parse_monitor_id,
};

/// Creates a notification channel on a monitor.
///
/// `POST /api/v1/monitors/{monitor_id}/notifications`
///
/// Requires the monitor to be owned by the caller — enforced twice: once
/// here (via `MonitorRepository::find_by_id_for_user` inside the use
/// case) and again at the SQL level by
/// `NotificationChannelRepository::create` writing a row whose
/// `notification_channels.user_id` matches, so an attacker can never
/// attach a channel to a monitor they don't own even if one of the two
/// checks had a bug.
pub async fn create_notification_channel(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(monitor_id): Path<String>,
    Json(payload): Json<CreateNotificationChannelRequest>,
) -> Result<(StatusCode, Json<NotificationChannelCreateResponse>), ApiError> {
    info!("API: create_notification_channel");

    let monitor_id = parse_monitor_id(&monitor_id)?;

    let use_case = CreateNotificationChannelUseCase::new(
        state.monitor_repository.clone(),
        state.notification_channel_repository.clone(),
        Arc::new(state.id_generator.clone()),
    );

    let channel = use_case
        .execute(CreateNotificationChannel {
            monitor_id,
            user_id,
            kind: payload.kind,
            target: payload.target,
        })
        .await
        .map_err(|err| match err {
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
