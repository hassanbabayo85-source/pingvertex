use axum::{extract::State, http::StatusCode};
use tracing::warn;

use crate::state::AppState;

/// `GET /health` — liveness. Deliberately zero-dependency: this only
/// proves the process itself is up and able to handle a request at all.
/// It must NEVER touch the database or any other external dependency —
/// an orchestrator uses liveness failures to decide whether to KILL and
/// restart the container, which is the wrong response to "the database
/// is temporarily unreachable" (that's what readiness, below, is for).
pub async fn health() -> StatusCode {
    StatusCode::OK
}

/// `GET /ready` — readiness.
///
/// Confirms the API can actually serve requests right now, starting with
/// the one dependency every route ultimately needs: the database. An
/// orchestrator uses readiness failures to decide whether to STOP ROUTING
/// traffic to this instance (without killing it) — the correct response
/// to a transient DB outage, since killing and restarting would not fix
/// anything and would just churn the fleet.
///
/// Returns `200 OK` if the check passes, `503 SERVICE_UNAVAILABLE`
/// otherwise. Never panics or leaks the underlying error to the response
/// body — only a generic status code, logged server-side.
pub async fn ready(State(state): State<AppState>) -> StatusCode {
    match state.health_check_port.check().await {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            warn!(error = %err, "readiness check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
