use std::time::Instant;

use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use application::{dto::ResendVerificationRequest, use_cases::ResendVerificationUseCase};

use crate::{auth_state::AuthState, auth_state::ResendVerificationRateLimiter};

/// Per-email fixed-window limit for `POST /auth/resend-verification`.
const RESEND_VERIFICATION_MAX_REQUESTS: u32 = 3;
const RESEND_VERIFICATION_WINDOW_SECS: u64 = 60;

async fn check_and_record_resend_rate_limit(
    limiter: &ResendVerificationRateLimiter,
    key: &str,
) -> bool {
    let mut buckets = limiter.lock().await;
    let now = Instant::now();

    match buckets.get_mut(key) {
        Some((window_start, count)) => {
            if now.duration_since(*window_start).as_secs() >= RESEND_VERIFICATION_WINDOW_SECS {
                *window_start = now;
                *count = 1;
                true
            } else if *count >= RESEND_VERIFICATION_MAX_REQUESTS {
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

    if !check_and_record_resend_rate_limit(
        &state.resend_verification_rate_limiter,
        &normalized_email,
    )
    .await
    {
        tracing::warn!("resend verification rate limit exceeded for a normalized email");
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
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn empty_limiter() -> ResendVerificationRateLimiter {
        Arc::new(Mutex::new(std::collections::HashMap::new()))
    }

    #[tokio::test]
    async fn allows_up_to_max_requests() {
        let limiter = empty_limiter();
        for _ in 0..RESEND_VERIFICATION_MAX_REQUESTS {
            assert!(check_and_record_resend_rate_limit(&limiter, "a@example.com").await);
        }
        assert!(!check_and_record_resend_rate_limit(&limiter, "a@example.com").await);
    }

    #[tokio::test]
    async fn different_emails_have_independent_limits() {
        let limiter = empty_limiter();
        for _ in 0..RESEND_VERIFICATION_MAX_REQUESTS {
            assert!(check_and_record_resend_rate_limit(&limiter, "a@example.com").await);
        }
        assert!(check_and_record_resend_rate_limit(&limiter, "b@example.com").await);
    }

    #[tokio::test]
    async fn window_resets_after_elapsed_time() {
        let limiter = empty_limiter();
        for _ in 0..RESEND_VERIFICATION_MAX_REQUESTS {
            assert!(check_and_record_resend_rate_limit(&limiter, "a@example.com").await);
        }

        // Manually age the window.
        {
            let mut buckets = limiter.lock().await;
            let (start, _) = buckets.get_mut("a@example.com").unwrap();
            *start = Instant::now()
                - std::time::Duration::from_secs(RESEND_VERIFICATION_WINDOW_SECS + 1);
        }

        assert!(check_and_record_resend_rate_limit(&limiter, "a@example.com").await);
    }
}
