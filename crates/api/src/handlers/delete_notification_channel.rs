use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::info;

use application::{
    commands::DeleteNotificationChannel, use_cases::DeleteNotificationChannelUseCase,
};
use domain::shared::ids::UserId;

use crate::{error::ApiError, state::AppState, validation::parse_notification_channel_id};

/// Deletes a notification channel (owner only).
///
/// `DELETE /api/v1/notifications/{id}`
pub async fn delete_notification_channel(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: delete_notification_channel");

    let id = parse_notification_channel_id(&id)?;

    let use_case =
        DeleteNotificationChannelUseCase::new(state.notification_channel_repository.clone());

    let deleted = use_case
        .execute(DeleteNotificationChannel { id, user_id })
        .await
        .map_err(|err| {
            ApiError::internal_with_source("failed to delete notification channel", err)
        })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        // Same response whether the channel never existed or belongs to
        // someone else — see `NotificationChannelRepository::delete_for_user`.
        Err(ApiError::not_found("notification channel not found"))
    }
}
