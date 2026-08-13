use axum::{
    extract::{Json, State},
    http::{
        header::{HeaderMap, HeaderValue, SET_COOKIE},
        StatusCode,
    },
};
use serde_json::{json, Value};

use application::{dto::RegisterRequest, use_cases::RegisterUserUseCase};

use tracing::info;

use crate::{auth_state::AuthState, error::ApiError};

/// Registers a new user.
pub async fn register(
    State(state): State<AuthState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), ApiError> {
    info!("API: register");

    let use_case = RegisterUserUseCase::new(
        state.user_repository.clone(),
        state.id_generator,
        state.password_hasher.clone(),
        state.token_service.clone(),
    );

    let response = use_case.execute(payload).await.map_err(|err| {
        tracing::warn!(error = %err, "registration failed");
        ApiError::bad_request("registration failed")
    })?;

    // Iri daya da login: an cire access_token daga JSON body domin ba a
    // fallasa raw JWT din ga JavaScript ba (misali ta hanyar XSS). httpOnly
    // cookie din shi ne kawai tushen gaskiya yanzu.
    let cookie = format!(
        "access_token={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400",
        response.access_token
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ApiError::internal("failed to set cookie"))?,
    );

    Ok((
        StatusCode::CREATED,
        headers,
        Json(json!({ "authenticated": true })),
    ))
}
