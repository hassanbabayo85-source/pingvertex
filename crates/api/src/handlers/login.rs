use axum::extract::{Json, State};
use axum::http::header::{HeaderMap, HeaderValue, SET_COOKIE};
use serde_json::{Value, json};

use application::{
    dto::{LoginOutcome, LoginRequest},
    use_cases::LoginUserUseCase,
};

use crate::{auth_state::AuthState, error::ApiError};

const LOGIN_LOCKOUT_MESSAGE: &str =
    "account temporarily locked due to too many failed login attempts; please try again later";

/// The one and only body this handler ever returns for an authentication
/// failure (other than the lockout 429). Kept as a single literal so it
/// can never drift into a per-case message that would leak account state.
const GENERIC_AUTH_FAILURE: &str = "invalid email or password";

pub async fn login(
    State(state): State<AuthState>,
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    let use_case = LoginUserUseCase::new(
        state.user_repository.clone(),
        state.password_hasher.clone(),
        state.token_service.clone(),
        state.id_generator.clone(),
        state.pending_auth_cache.clone(),
    );

    let outcome = use_case.execute(payload).await.map_err(|err| {
        let message = err.to_string();
        tracing::warn!(error = %message, "login failed");

        if message == LOGIN_LOCKOUT_MESSAGE {
            ApiError::too_many_requests(message)
        } else {
            ApiError::bad_request(GENERIC_AUTH_FAILURE)
        }
    })?;

    match outcome {
        LoginOutcome::PendingTwoFactor(pending) => Ok((
            HeaderMap::new(),
            Json(json!({
                "authenticated": false,
                "two_factor_required": true,
                "pending_token": pending.pending_token,
            })),
        )),
        LoginOutcome::Authenticated(response) => {
            let cookie = shared::cookie::access_token_cookie(&response.access_token);

            let mut headers = HeaderMap::new();
            headers.insert(
                SET_COOKIE,
                HeaderValue::from_str(&cookie)
                    .map_err(|_| ApiError::internal("failed to set cookie"))?,
            );

            Ok((headers, Json(json!({ "authenticated": true }))))
        }
    }
}
