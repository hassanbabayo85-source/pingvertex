use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::info;

use application::ports::check_repository::CheckFilters;
use application::use_cases::ListChecksUseCase;

use crate::{
    error::ApiError,
    handlers::types::CheckResponse,
    middleware::api_key_auth::ApiKeyPrincipal,
    state::AppState,
    validation::{parse_check_status, parse_datetime, parse_monitor_id, validate_pagination},
};

#[derive(Deserialize)]
pub struct ListChecksViaApiKeyQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_offset")]
    offset: u32,
    monitor_id: Option<String>,
    status: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

fn default_limit() -> u32 {
    10
}
fn default_offset() -> u32 {
    0
}

fn parse_filters(query: &ListChecksViaApiKeyQuery) -> Result<CheckFilters, ApiError> {
    let monitor_id = match &query.monitor_id {
        Some(raw) => Some(parse_monitor_id(raw)?),
        None => None,
    };

    let status = match &query.status {
        Some(raw) => Some(parse_check_status(raw)?),
        None => None,
    };

    let from = match &query.from {
        Some(raw) => Some(parse_datetime(raw)?),
        None => None,
    };

    let to = match &query.to {
        Some(raw) => Some(parse_datetime(raw)?),
        None => None,
    };

    Ok(CheckFilters {
        monitor_id,
        status,
        from,
        to,
    })
}

/// Lists checks for the tenant that owns the API key used to authenticate
/// this request.
///
/// `GET /api/v1/external/checks`
///
/// Gated on `ApiKeyScope::MonitorRead`. There is no dedicated `check:*`
/// scope in `ApiKeyScope` (one was named, `check:read`, but the
/// shipped enum from the earlier, undocumented API-key worker does not
/// have it — see the API key scope notes for why this
/// worker did not add one). Checks are always read as history attached
/// to a monitor, so reusing `MonitorRead` — rather than adding a new
/// scope string to the `api_keys.scopes` column's existing, in-database
/// contract — is the narrower, additive choice.
pub async fn list_checks_via_api_key(
    Query(query): Query<ListChecksViaApiKeyQuery>,
    Extension(principal): Extension<ApiKeyPrincipal>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("API: list_checks_via_api_key");

    if let Err(err) = validate_pagination(query.limit, query.offset) {
        return err.into_response();
    }

    let filters = match parse_filters(&query) {
        Ok(filters) => filters,
        Err(err) => return err.into_response(),
    };

    let use_case = ListChecksUseCase::new(
        state.check_repository.clone(),
        state.diagnostic_report_repository.clone(),
    );

    match use_case
        .execute_for_tenant(query.limit, query.offset, principal.tenant_id, filters)
        .await
    {
        Ok(checks) => {
            let response: Vec<CheckResponse> =
                checks.into_iter().map(CheckResponse::from).collect();
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(err) => ApiError::internal_with_source("failed to list checks", err).into_response(),
    }
}
