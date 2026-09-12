use axum::{
    Router,
    routing::{delete, post},
};

use crate::{
    handlers::{
        create_notification_channel, delete_notification_channel, list_notification_channels,
        test_notification_channel,
    },
    state::AppState,
};

/// Create/list/delete routes for notification channels.
///
/// Wrapped with the same standard write-rate limiter as `monitors`/`checks`
/// in `router.rs`. Kept separate from `test_router()` below because the
/// test-send endpoint needs a much stricter limit — `GovernorLayer` applies
/// to a whole `Router`, so the two limits can't share one router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/monitors/{monitor_id}/notifications",
            post(create_notification_channel).get(list_notification_channels),
        )
        .route("/notifications/{id}", delete(delete_notification_channel))
}

/// The "send test notification" route, kept on its own `Router` so
/// `router.rs` can wrap it with a much stricter rate limit than ordinary
/// writes. Without a tight limit here, an authenticated user could point a
/// webhook channel at a third party's URL and use this server as a
/// spam/DoS source against it.
pub fn test_router() -> Router<AppState> {
    Router::new().route("/notifications/{id}/test", post(test_notification_channel))
}
