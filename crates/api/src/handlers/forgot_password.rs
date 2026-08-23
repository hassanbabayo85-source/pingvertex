use std::time::Instant;

use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{dto::ForgotPasswordRequest, use_cases::RequestPasswordResetUseCase};

use crate::auth_state::{AuthState, ForgotPasswordRateLimiter};

/// Per-email fixed-window limit for `POST /auth/forgot-password`.
/// Identical shape/values to `resend_verification.rs`'s limiter — same
/// abuse profile (an attacker could otherwise email-bomb a victim by
/// repeatedly triggering reset emails for their address).
const FORGOT_PASSWORD_MAX_REQUESTS: u32 = 3;
const FORGOT_PASSWORD_WINDOW_SECS: u64 = 60;

async fn check_and_record_forgot_password_rate_limit(
    limiter: &ForgotPasswordRateLimiter,
    key: &str,
) -> bool {
    let mut buckets = limiter.lock().await;
    let now = Instant::now();

    match buckets.get_mut(key) {
        Some((window_start, count)) => {
            if now.duration_since(*window_start).as_secs() >= FORGOT_PASSWORD_WINDOW_SECS {
                *window_start = now;
                *count = 1;
                true
            } else if *count >= FORGOT_PASSWORD_MAX_REQUESTS {
                false
            } else {
                *count += 1;
                true
            }
        }
        None => {
            buckets.insert(key.to_string(), (now, 1));
            true
        }
    }
}

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

    if !check_and_record_forgot_password_rate_limit(
        &state.forgot_password_rate_limiter,
        &normalized_email,
    )
    .await
    {
        tracing::warn!("forgot-password rate limit exceeded for a normalized email");
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
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn empty_limiter() -> ForgotPasswordRateLimiter {
        Arc::new(Mutex::new(std::collections::HashMap::new()))
    }

    #[tokio::test]
    async fn allows_up_to_max_requests() {
        let limiter = empty_limiter();
        for _ in 0..FORGOT_PASSWORD_MAX_REQUESTS {
            assert!(check_and_record_forgot_password_rate_limit(&limiter, "a@example.com").await);
        }
        assert!(!check_and_record_forgot_password_rate_limit(&limiter, "a@example.com").await);
    }

    #[tokio::test]
    async fn different_emails_have_independent_limits() {
        let limiter = empty_limiter();
        for _ in 0..FORGOT_PASSWORD_MAX_REQUESTS {
            assert!(check_and_record_forgot_password_rate_limit(&limiter, "a@example.com").await);
        }
        assert!(check_and_record_forgot_password_rate_limit(&limiter, "b@example.com").await);
    }

    #[tokio::test]
    async fn window_resets_after_elapsed_time() {
        let limiter = empty_limiter();
        for _ in 0..FORGOT_PASSWORD_MAX_REQUESTS {
            assert!(check_and_record_forgot_password_rate_limit(&limiter, "a@example.com").await);
        }

        {
            let mut buckets = limiter.lock().await;
            let (start, _) = buckets.get_mut("a@example.com").unwrap();
            *start =
                Instant::now() - std::time::Duration::from_secs(FORGOT_PASSWORD_WINDOW_SECS + 1);
        }

        assert!(check_and_record_forgot_password_rate_limit(&limiter, "a@example.com").await);
    }
}
