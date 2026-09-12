use axum::{
    extract::{Json, State},
    http::{StatusCode, header::HeaderMap},
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
    let email_key = registration_email.trim().to_ascii_lowercase();

    // Registration is an abuse-sensitive endpoint even with the global IP
    // limiter. A distributed per-email limiter prevents attackers from
    // repeatedly targeting the same mailbox or SMTP provider through
    // rotating source IPs.
    let allowed = state
        .rate_limiter
        .check_and_record("register", &email_key, 3, 600)
        .await
        .map_err(|_| ApiError::internal("registration service unavailable"))?;
    if !allowed {
        return Err(ApiError::too_many_requests(
            "too many registration attempts; please try again later",
        ));
    }

    let use_case = RegisterUserUseCase::new(
        state.user_repository.clone(),
        state.id_generator.clone(),
        state.password_hasher.clone(),
        state.registration_transaction.clone(),
    );

    use_case.execute(payload).await.map_err(|err| {
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

    // SECURITY: registration never issues a JWT. The account remains
    // PendingVerification until the 6-digit OTP is successfully redeemed.
    Ok((
        StatusCode::CREATED,
        HeaderMap::new(),
        Json(json!({
            "authenticated": false,
            "verification_required": true,
            "message": "account created; enter the verification code sent to your email"
        })),
    ))
}
