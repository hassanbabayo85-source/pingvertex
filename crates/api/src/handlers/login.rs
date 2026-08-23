use axum::extract::{Json, State};
use axum::http::header::{HeaderMap, HeaderValue, SET_COOKIE};
use serde_json::{Value, json};

use application::{
    dto::{LoginOutcome, LoginRequest},
    use_cases::LoginUserUseCase,
};

use crate::{
    auth_state::AuthState, // Mun yi amfani da AuthState a maimakon AppState
    error::ApiError,
};

/// Must match `LOGIN_LOCKOUT_MESSAGE` in
/// `crates/application/src/use_cases/login_user.rs` EXACTLY. Duplicated
/// here (rather than exported cross-crate) to avoid adding a new public
/// item/re-export to `application`'s `use_cases` module just for this one
/// string comparison. If the two literals are ever edited out of sync,
/// this fails SAFE — the login is still refused either way; the lockout
/// case just falls back to a 400 with the generic message instead of a
/// 429, it never leaks anything and never bypasses the lockout itself
/// (the use case refuses the login regardless of what this handler does
/// with the resulting error).
const LOGIN_LOCKOUT_MESSAGE: &str =
    "account temporarily locked due to too many failed login attempts; please try again later";

pub async fn login(
    State(state): State<AuthState>, // AuthState ake bukata a nan
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
        // SEC (B4 audit): every login failure is logged server-side with
        // its real cause, but the client-facing message is ALWAYS the
        // same generic string — except the lockout case, which is itself
        // generic across existing/nonexistent accounts (see
        // `login_user.rs`) — never the use case's own error text.
        // Forwarding `err.to_string()` straight to the client here
        // previously leaked e.g. a distinct "account suspended" message
        // (account-status enumeration) and, on a cache failure, raw
        // internal error text.
        let message = err.to_string();
        tracing::warn!(error = %message, "login failed");

        if message == LOGIN_LOCKOUT_MESSAGE {
            ApiError::too_many_requests(message)
        } else {
            ApiError::bad_request("invalid email or password")
        }
    })?;

    match outcome {
        // IDAN 2FA na aiki a wannan account, BABU cookie da za a saita
        // tukuna — client dole ya kammala `POST /auth/totp/verify` da
        // `pending_token` din nan kafin ya samu ainihin session JWT.
        LoginOutcome::PendingTwoFactor(pending) => Ok((
            HeaderMap::new(),
            Json(json!({
                "authenticated": false,
                "two_factor_required": true,
                "pending_token": pending.pending_token,
            })),
        )),
        LoginOutcome::Authenticated(response) => {
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
                HeaderValue::from_str(&cookie)
                    .map_err(|_| ApiError::internal("failed to set cookie"))?,
            );

            Ok((headers, Json(json!({ "authenticated": true }))))
        }
    }
}
