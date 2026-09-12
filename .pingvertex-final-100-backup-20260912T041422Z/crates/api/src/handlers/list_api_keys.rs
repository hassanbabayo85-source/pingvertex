use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use tracing::info;

use application::use_cases::ListApiKeysUseCase;
use domain::shared::types::AuthenticatedUser;

use crate::{error::ApiError, handlers::api_key_types::ApiKeySummaryResponse, state::AppState};

/// Lists every API key belonging to the caller's tenant.
///
/// `GET /api/v1/api-keys`
pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<(StatusCode, Json<Vec<ApiKeySummaryResponse>>), ApiError> {
    info!("API: list_api_keys");

    let use_case = ListApiKeysUseCase::new(state.api_key_repository.clone());

    let keys = use_case
        .execute(authenticated_user)
        .await
        .map_err(|err| ApiError::internal_with_source("failed to list api keys", err))?;

    let response: Vec<ApiKeySummaryResponse> =
        keys.into_iter().map(ApiKeySummaryResponse::from).collect();

    Ok((StatusCode::OK, Json(response)))
}
