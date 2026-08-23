use std::sync::Arc;

use axum::{Json, extract::Extension, extract::State, http::StatusCode};
use tracing::info;

use application::{
    commands::CreateApiKey,
    use_cases::{CreateApiKeyError, CreateApiKeyUseCase},
};
use domain::shared::types::AuthenticatedUser;

use crate::{
    error::ApiError,
    handlers::api_key_types::{ApiKeyCreateResponse, CreateApiKeyRequest},
    state::AppState,
};

/// Creates a scoped API key for the caller's tenant.
///
/// `POST /api/v1/api-keys`
///
/// Returns the raw key exactly once — see `ApiKeyCreateResponse`'s doc
/// comment.
pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiKeyCreateResponse>), ApiError> {
    info!("API: create_api_key");

    let scopes = payload.parse_scopes()?;

    let use_case = CreateApiKeyUseCase::new(
        state.api_key_repository.clone(),
        Arc::new(state.id_generator.clone()),
        state.password_hasher.clone(),
        state.audit_log_entry_repository.clone(),
    );

    let created = use_case
        .execute(
            CreateApiKey {
                name: payload.name,
                scopes,
                expires_at: payload.expires_at,
            },
            authenticated_user,
        )
        .await
        .map_err(|err| match err {
            CreateApiKeyError::Forbidden => ApiError::forbidden(err.to_string()),
            CreateApiKeyError::EmptyName
            | CreateApiKeyError::NameTooLong
            | CreateApiKeyError::EmptyScopes
            | CreateApiKeyError::ExpiryInPast => ApiError::bad_request(err.to_string()),
            CreateApiKeyError::Other(err) => {
                ApiError::internal_with_source("failed to create api key", err)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreateResponse::new(created.api_key, created.raw_key)),
    ))
}
