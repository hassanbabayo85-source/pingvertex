use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{dto::ResendVerificationRequest, use_cases::ResendVerificationUseCase};

use crate::auth_state::AuthState;

/// Per-email fixed-window limit for `POST /auth/resend-verification`.
const RESEND_VERIFICATION_MAX_REQUESTS: u32 = 3;
const RESEND_VERIFICATION_WINDOW_SECS: u64 = 60;

/// `POST /auth/resend-verification`
///
/// Enumeration/abuse protection:
/// - A fixed-window per-email limiter prevents hammering a single target.
/// - The response body is IDENTICAL whether the email is invalid,
///   nonexistent, unverified, or already verified.
/// - Internal errors are logged but NOT surfaced, so they cannot be used
///   as an account-existence oracle.
pub async fn resend_verification(
    State(state): State<AuthState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> (StatusCode, Json<Value>) {
    let normalized_email = payload.email.trim().to_ascii_lowercase();

    let allowed = match state
        .rate_limiter
        .check_and_record(
            "resend_verification",
            &normalized_email,
            RESEND_VERIFICATION_MAX_REQUESTS,
            RESEND_VERIFICATION_WINDOW_SECS,
        )
        .await
    {
        Ok(allowed) => allowed,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "resend-verification rate limiter unavailable; failing closed"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "message": "verification email service is temporarily unavailable; please try again later"
                })),
            );
        }
    };

    if !allowed {
        tracing::warn!("resend verification rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "message": "too many verification emails requested; please try again later"
            })),
        );
    }

    let use_case = ResendVerificationUseCase::new(
        state.user_repository.clone(),
        state.email_verification_token_repository.clone(),
        state.id_generator.clone(),
        state.password_hasher.clone(),
        state.email_verification_mailer.clone(),
    );

    if let Err(err) = use_case.execute(payload).await {
        // Do NOT log any raw token. The error here is only internal
        // repository/mailer failure — never the user's token.
        tracing::warn!(error = %err, "resend verification failed");
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "message": "if an account with that email address exists and is not verified, a verification email has been sent"
        })),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalized_email_is_trimmed_and_lowercase() {
        let email = "  User@Example.COM ";
        assert_eq!(email.trim().to_ascii_lowercase(), "user@example.com");
    }

    #[test]
    fn rate_limit_configuration_is_sixty_seconds_and_three_requests() {
        assert_eq!(super::RESEND_VERIFICATION_MAX_REQUESTS, 3);
        assert_eq!(super::RESEND_VERIFICATION_WINDOW_SECS, 60);
    }
}
