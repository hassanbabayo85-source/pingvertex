use axum::extract::{Json, State};
use axum::http::header::{HeaderMap, HeaderValue, SET_COOKIE};
use serde_json::{Value, json};

use application::{dto::VerifyTotpRequest, use_cases::VerifyTotpAndLoginUseCase};

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /auth/totp/verify` — deliberately NOT behind `jwt_auth`: the
/// caller does not have a session JWT yet at this point in the login
/// flow, only the short-lived `pending_token` `POST /auth/login`
/// returned. Rate-limited far more tightly than the rest of `/auth/*`
/// (see the dedicated `GovernorLayer` in `router.rs`) since this is the
/// endpoint that guesses a 6-digit TOTP code would target.
pub async fn verify_totp(
    State(state): State<AuthState>,
    Json(payload): Json<VerifyTotpRequest>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let use_case = VerifyTotpAndLoginUseCase::new(
        state.user_repository.clone(),
        state.secret_cipher.clone(),
        state.password_hasher.clone(),
        state.token_service.clone(),
        state.pending_auth_cache.clone(),
        state.backup_code_repository.clone(),
    );

    let response = use_case
        .execute(payload)
        .await
        .map_err(|err| ApiError::unauthorized(err.to_string()))?;

    // Iri daya da `login.rs`: JWT yana zaune KAWAI a cikin httpOnly
    // cookie, ba a cikin JSON body ba.
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
