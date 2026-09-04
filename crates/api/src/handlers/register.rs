use axum::{
    extract::{Json, State},
    http::{
        StatusCode,
        header::{HeaderMap, HeaderValue, SET_COOKIE},
    },
};
use serde_json::{Value, json};

use application::{
    dto::RegisterRequest,
    use_cases::{CreateEmailVerificationTokenUseCase, RegisterUserUseCase},
};

use domain::Email;
use tracing::info;

use crate::{auth_state::AuthState, error::ApiError};

pub async fn register(
    State(state): State<AuthState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), ApiError> {
    info!("API: register");

    let registration_email = payload.email.clone();

    let use_case = RegisterUserUseCase::new(
        state.user_repository.clone(),
        state.id_generator.clone(),
        state.password_hasher.clone(),
        state.token_service.clone(),
        state.registration_transaction.clone(),
    );

    let response = use_case.execute(payload).await.map_err(|err| {
        tracing::warn!(error = %err, "registration failed");
        ApiError::bad_request("registration failed")
    })?;

    // Best-effort verification email. Registration itself succeeds even
    // if SMTP is unavailable; the user can re-request via
    // `POST /auth/resend-verification`.
    if let Ok(email) = Email::new(registration_email) {
        match state.user_repository.find_by_email(&email).await {
            Ok(Some(user)) => {
                let create_token = CreateEmailVerificationTokenUseCase::new(
                    state.email_verification_token_repository.clone(),
                    state.id_generator.clone(),
                    state.password_hasher.clone(),
                    state.email_verification_mailer.clone(),
                );

                if let Err(err) = create_token.execute(user.id(), &email).await {
                    tracing::warn!(error = %err, "failed to send verification email after registration");
                }
            }
            Ok(None) => {
                tracing::warn!("registered user not found while sending verification email");
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to look up registered user for verification email");
            }
        }
    }

    let cookie = shared::cookie::access_token_cookie(&response.access_token);

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
