use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use application::use_cases::{ListNotificationChannelsError, ListNotificationChannelsUseCase};
use domain::shared::ids::UserId;

use crate::{
    error::ApiError, handlers::types::NotificationChannelResponse, state::AppState,
    validation::parse_monitor_id,
};

/// Lists notification channels configured on a monitor (owner only).
///
/// `GET /api/v1/monitors/{monitor_id}/notifications`
pub async fn list_notification_channels(
    Path(monitor_id): Path<String>,
    Extension(user_id): Extension<UserId>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_notification_channels");

    let monitor_id = match parse_monitor_id(&monitor_id) {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    let use_case = ListNotificationChannelsUseCase::new(
        state.monitor_repository.clone(),
        state.notification_channel_repository.clone(),
    );

    match use_case.execute(monitor_id, user_id).await {
        Ok(channels) => {
            let response: Vec<NotificationChannelResponse> = channels
                .into_iter()
                .map(NotificationChannelResponse::from)
                .collect();

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(ListNotificationChannelsError::NotFound) => {
            ApiError::not_found("monitor not found").into_response()
        }
        Err(ListNotificationChannelsError::Other(err)) => {
            ApiError::internal_with_source("failed to list notification channels", err)
                .into_response()
        }
    }
}
