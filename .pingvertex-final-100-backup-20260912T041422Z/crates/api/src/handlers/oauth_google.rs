use axum::extract::{Json, State};
use axum::http::header::{HeaderMap, HeaderValue, SET_COOKIE};
use serde_json::{Value, json};

use application::{dto::OAuthLoginRequest, ports::IdTokenVerifier, use_cases::OAuthLoginUseCase};

use crate::{auth_state::AuthState, error::ApiError};

/// `POST /auth/oauth/google` — exchanges a Google Identity Services
/// `id_token` (the frontend's `credential` field) for a PingVertex
/// session. Signs the caller into an existing account matched by
/// verified email, or provisions a brand-new account+tenant the first
/// time this email is seen — see `OAuthLoginUseCase`.
pub async fn oauth_google(
    State(state): State<AuthState>,
    Json(payload): Json<OAuthLoginRequest>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let identity = state
        .google_id_token_verifier
        .verify(&payload.id_token)
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "google id_token verification failed");
            ApiError::unauthorized("could not verify Google sign-in")
        })?;

    let use_case = OAuthLoginUseCase::new(
        state.user_repository.clone(),
        state.id_generator.clone(),
        state.password_hasher.clone(),
        state.token_service.clone(),
        state.registration_transaction.clone(),
        state.oauth_identity_repository.clone(),
        "google",
    );

    let response = use_case.execute(identity).await.map_err(|err| {
        tracing::warn!(error = %err, "google sign-in failed");
        ApiError::bad_request("google sign-in failed")
    })?;

    let cookie = shared::cookie::access_token_cookie(&response.access_token);

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ApiError::internal("failed to set cookie"))?,
    );

    Ok((headers, Json(json!({ "authenticated": true }))))
}
