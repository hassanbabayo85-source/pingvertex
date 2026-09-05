use axum::{
    Router,
    routing::{get, post},
};

use crate::handlers::{
    change_password::change_password, confirm_totp::confirm_totp, current_session::current_session,
    disable_totp::disable_totp, enable_totp::enable_totp, forgot_password::forgot_password,
    login::login, logout::logout, oauth_apple::oauth_apple, oauth_google::oauth_google,
    register::register, resend_verification::resend_verification, reset_password::reset_password,
    verify_email::verify_email, verify_totp::verify_totp,
};

pub fn router() -> Router<crate::auth_state::AuthState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/oauth/google", post(oauth_google))
        .route("/oauth/apple", post(oauth_apple))
        .route("/verify-email", post(verify_email))
        .route("/verify-email-otp", post(verify_email_otp))
        .route("/resend-verification", post(resend_verification))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
}

pub fn totp_verify_router() -> Router<crate::auth_state::AuthState> {
    Router::new().route("/totp/verify", post(verify_totp))
}

/// Authentication-protected account endpoints. `jwt_auth` is applied by the
/// top-level router before this router is mounted.
pub fn protected_router() -> Router<crate::auth_state::AuthState> {
    Router::new()
        .route("/logout", post(logout))
        .route("/me", get(current_session))
        .route("/totp/enable", post(enable_totp))
        .route("/totp/confirm", post(confirm_totp))
        .route("/totp/disable", post(disable_totp))
        .route("/password/change", post(change_password))
}
