use axum::extract::{Json, State};
use axum::http::header::{HeaderMap, HeaderValue, SET_COOKIE};
use serde_json::{json, Value};

use application::{dto::LoginRequest, use_cases::LoginUserUseCase};

use crate::{
    auth_state::AuthState, // Mun yi amfani da AuthState a maimakon AppState
    error::ApiError,
};

pub async fn login(
    State(state): State<AuthState>, // AuthState ake bukata a nan
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let use_case = LoginUserUseCase::new(
        state.user_repository.clone(),
        state.password_hasher.clone(),
        state.token_service.clone(),
    );

    let response = use_case
        .execute(payload)
        .await
        .map_err(|err| ApiError::bad_request(err.to_string()))?; // Yayi daidai da yadda mukayi a register.rs

    // Sirrin JWT yana zaune KAWAI a cikin httpOnly cookie yanzu — an cire shi
    // daga JSON body domin JSON response din yana iya bayyana ga JavaScript
    // (misali ta hanyar XSS a wani sashe na frontend), wanda zai soke fa'idar
    // amfani da httpOnly cookie tun farko. Dashboard din (apps/dashboard) ba
    // ya karanta access_token daga JSON body ko ta yaya — yana dogara ne kawai
    // ga cookie din da kuma wani boolean flag na gida.
    let cookie = format!(
        "access_token={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=86400",
        response.access_token
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ApiError::internal("failed to set cookie"))?,
    );

    Ok((headers, Json(json!({ "authenticated": true }))))
}
