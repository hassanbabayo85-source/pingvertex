use axum::extract::{Json, State};

use application::{dto::LoginRequest, use_cases::LoginUserUseCase};

use crate::{
    auth_state::AuthState, // Mun yi amfani da AuthState a maimakon AppState
    error::ApiError,
};

pub async fn login(
    State(state): State<AuthState>, // AuthState ake bukata a nan
    Json(payload): Json<LoginRequest>,
) -> Result<Json<application::dto::AuthResponse>, ApiError> {
    let use_case = LoginUserUseCase::new(
        state.user_repository.clone(),
        state.password_hasher.clone(),
        state.token_service.clone(),
    );

    let response = use_case
        .execute(payload)
        .await
        .map_err(|err| ApiError::bad_request(err.to_string()))?; // Yayi daidai da yadda mukayi a register.rs

    Ok(Json(response))
}
