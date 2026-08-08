use axum::{
    extract::{Json, State},
    http::StatusCode,
};

use application::{dto::RegisterRequest, use_cases::RegisterUserUseCase};

use tracing::info;

use crate::{auth_state::AuthState, error::ApiError};

/// Registers a new user.
pub async fn register(
    State(state): State<AuthState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<application::dto::AuthResponse>), ApiError> {
    info!("API: register");

    let use_case = RegisterUserUseCase::new(
        state.user_repository.clone(),
        state.id_generator,
        state.password_hasher.clone(),
        state.token_service.clone(),
    );

    let response = use_case
        .execute(payload)
        .await
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

    Ok((StatusCode::CREATED, Json(response)))
}
