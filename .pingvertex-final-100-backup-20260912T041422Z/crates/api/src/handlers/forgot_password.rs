use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{dto::ForgotPasswordRequest, use_cases::RequestPasswordResetUseCase};

use crate::auth_state::AuthState;

/// Per-email fixed-window limit for `POST /auth/forgot-password`.
/// Identical shape/values to `resend_verification.rs`'s limiter — same
/// abuse profile (an attacker could otherwise email-bomb a victim by
/// repeatedly triggering reset emails for their address).
const FORGOT_PASSWORD_MAX_REQUESTS: u32 = 3;
const FORGOT_PASSWORD_WINDOW_SECS: u64 = 60;

/// `POST /auth/forgot-password`
///
/// Enumeration/abuse protection:
/// - A fixed-window per-email limiter prevents hammering a single target
///   with reset emails.
/// - The response body is IDENTICAL whether the email is malformed,
///   nonexistent, or belongs to a real account.
/// - Internal errors are logged but NOT surfaced, so they cannot be used
///   as an account-existence oracle.
///
/// Also covered by the shared IP-based `GovernorLayer` applied to the
/// whole `/auth/*` group in `router.rs` (5 requests/60s/IP).
pub async fn forgot_password(
    State(state): State<AuthState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> (StatusCode, Json<Value>) {
    let normalized_email = payload.email.trim().to_ascii_lowercase();

    let allowed = match state
        .rate_limiter
        .check_and_record(
            "forgot_password",
            &normalized_email,
            FORGOT_PASSWORD_MAX_REQUESTS,
            FORGOT_PASSWORD_WINDOW_SECS,
        )
        .await
    {
        Ok(allowed) => allowed,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "forgot-password rate limiter unavailable; failing closed"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "message": "password reset is temporarily unavailable; please try again later"
                })),
            );
        }
    };

    if !allowed {
        tracing::warn!("forgot-password rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "message": "too many password reset requests; please try again later"
            })),
        );
    }

    let use_case = RequestPasswordResetUseCase::new(
        state.user_repository.clone(),
        state.reset_password_token_repository.clone(),
        state.id_generator.clone(),
        state.password_hasher.clone(),
        state.password_reset_mailer.clone(),
    );

    if let Err(err) = use_case.execute(payload).await {
        // Do NOT log any raw token. The error here is only internal
        // repository/mailer failure — never the user's token.
        tracing::warn!(error = %err, "forgot-password request failed");
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "message": "if an account with that email address exists, a password reset email has been sent"
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
        assert_eq!(super::FORGOT_PASSWORD_MAX_REQUESTS, 3);
        assert_eq!(super::FORGOT_PASSWORD_WINDOW_SECS, 60);
    }
}
