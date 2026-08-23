use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use tracing::info;

use crate::{
    error::ApiError, handlers::types::MonitorResponse, middleware::api_key_auth::ApiKeyPrincipal,
    state::AppState,
};

/// Lists monitors for the tenant that owns the API key used to
/// authenticate this request.
///
/// `GET /api/v1/external/monitors`
///
/// Authenticated via `ApiKeyAuthMiddleware` + `require_api_key_scope`
/// (wired in `router.rs`) instead of `jwt_auth` — this is the reference
/// endpoint demonstrating end-to-end `X-API-Key` authentication with
/// `ApiKeyScope::MonitorRead` enforcement. A key with only, say,
/// `StatusPageRead` gets `403 FORBIDDEN` here even though the key itself
/// is valid.
pub async fn list_monitors_via_api_key(
    State(state): State<AppState>,
    Extension(principal): Extension<ApiKeyPrincipal>,
) -> Result<(StatusCode, Json<Vec<MonitorResponse>>), ApiError> {
    info!("API: list_monitors_via_api_key");

    let monitors = state
        .monitor_repository
        .find_all_for_tenant(principal.tenant_id)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list monitors", err))?;

    let response: Vec<MonitorResponse> = monitors.into_iter().map(MonitorResponse::from).collect();

    Ok((StatusCode::OK, Json(response)))
}
