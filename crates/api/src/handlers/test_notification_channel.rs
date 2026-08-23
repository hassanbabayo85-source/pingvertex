use axum::{
    Json,
    extract::{Extension, Path, State},
};
use serde::Serialize;
use tracing::info;

use application::use_cases::{SendTestNotificationError, SendTestNotificationUseCase};
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, state::AppState, validation::parse_notification_channel_id};

#[derive(Debug, Serialize)]
pub struct TestNotificationResponse {
    pub status: &'static str,
}

/// Sends a test notification to a single channel (owner only).
///
/// `POST /api/v1/notifications/{id}/test`
///
/// Rate-limited more strictly than other write endpoints (see
/// `router.rs`) — without a limit, this endpoint would let an
/// authenticated-but-malicious user turn this server into a spam/DoS
/// source against any third-party webhook URL of their choosing, simply
/// by creating a channel pointing at a victim's endpoint and hammering
/// "test" repeatedly.
///
/// Unlike incident notifications (which only ever log a failure — see
/// `worker::IncidentManager::notify_channels`), a failed test send IS
/// surfaced to the caller as an error: that's the entire point of a test
/// button.
pub async fn test_notification_channel(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<Json<TestNotificationResponse>, ApiError> {
    info!("API: test_notification_channel");

    let id = parse_notification_channel_id(&id)?;

    let use_case = SendTestNotificationUseCase::new(
        state.notification_channel_repository.clone(),
        state.monitor_repository.clone(),
        state.notification_service.clone(),
    );

    use_case
        .execute(id, authenticated_user)
        .await
        .map_err(|err| match err {
            SendTestNotificationError::ChannelNotFound => {
                ApiError::not_found("notification channel not found")
            }
            SendTestNotificationError::MonitorNotFound => ApiError::not_found("monitor not found"),
            SendTestNotificationError::DeliveryFailed(err) => {
                ApiError::bad_request(format!("failed to deliver test notification: {err}"))
            }
            SendTestNotificationError::Other(err) => {
                ApiError::internal_with_source("failed to send test notification", err)
            }
        })?;

    Ok(Json(TestNotificationResponse { status: "sent" }))
}
