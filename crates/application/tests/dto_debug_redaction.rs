//! Debug redaction regression tests (application DTOs).
//!
//! Every DTO that carries a password, JWT, TOTP code, backup code,
//! reset token, or OAuth id_token must render `[REDACTED]` for those
//! fields in `Debug` output. An accidental `tracing::info!(?request)`
//! would otherwise write credentials to logs.

use application::dto::{
    AuthResponse, ChangePasswordRequest, ConfirmTotpSetupRequest, ConfirmTotpSetupResponse,
    DisableTotpRequest, EnableTotpResponse, LoginRequest, OAuthLoginRequest,
    PendingTwoFactorResponse, RegisterRequest, ResetPasswordRequest, VerifyTotpRequest,
};

fn assert_no_plaintext(rendered: &str, needles: &[&str], what: &str) {
    for needle in needles {
        assert!(!needle.is_empty(), "test bug: empty needle");
        assert!(
            !rendered.contains(needle),
            "SECURITY: {what} leaked '{needle}' in Debug output: {rendered}"
        );
    }
}

fn assert_redacted(rendered: &str, what: &str) {
    assert!(
        rendered.contains("[REDACTED]"),
        "SECURITY: {what} did not emit '[REDACTED]' in Debug output: {rendered}"
    );
}

#[test]
fn auth_response_debug_redacts_access_token() {
    let r = AuthResponse {
        access_token: "jwt_THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(&rendered, &["jwt_THIS_MUST_NOT_LEAK"], "AuthResponse");
    assert_redacted(&rendered, "AuthResponse");
}

#[test]
fn login_request_debug_redacts_password() {
    let r = LoginRequest {
        email: "user@example.com".to_string(),
        password: "THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(&rendered, &["THIS_MUST_NOT_LEAK"], "LoginRequest");
    assert_redacted(&rendered, "LoginRequest");
    // Email is fine to show.
    assert!(rendered.contains("user@example.com"));
}

#[test]
fn register_request_debug_redacts_password() {
    let r = RegisterRequest {
        username: "newuser".to_string(),
        email: "new@example.com".to_string(),
        password: "THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(&rendered, &["THIS_MUST_NOT_LEAK"], "RegisterRequest");
    assert_redacted(&rendered, "RegisterRequest");
}

#[test]
fn reset_password_request_debug_redacts_token_and_new_password() {
    let r = ResetPasswordRequest {
        token: "rst_THIS_MUST_NOT_LEAK".to_string(),
        new_password: "ALSO_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(
        &rendered,
        &["rst_THIS_MUST_NOT_LEAK", "ALSO_MUST_NOT_LEAK"],
        "ResetPasswordRequest",
    );
    assert_redacted(&rendered, "ResetPasswordRequest");
}

#[test]
fn change_password_request_debug_redacts_both_passwords() {
    let r = ChangePasswordRequest {
        current_password: "OLD_THIS_MUST_NOT_LEAK".to_string(),
        new_password: "NEW_THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(
        &rendered,
        &["OLD_THIS_MUST_NOT_LEAK", "NEW_THIS_MUST_NOT_LEAK"],
        "ChangePasswordRequest",
    );
    assert_redacted(&rendered, "ChangePasswordRequest");
}

#[test]
fn verify_totp_request_debug_redacts_pending_token_and_code() {
    let r = VerifyTotpRequest {
        pending_token: "pending_THIS_MUST_NOT_LEAK".to_string(),
        code: "123456".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(
        &rendered,
        &["pending_THIS_MUST_NOT_LEAK", "123456"],
        "VerifyTotpRequest",
    );
    assert_redacted(&rendered, "VerifyTotpRequest");
}

#[test]
fn confirm_totp_setup_request_debug_redacts_code() {
    let r = ConfirmTotpSetupRequest {
        totp_code: "654321".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(&rendered, &["654321"], "ConfirmTotpSetupRequest");
    assert_redacted(&rendered, "ConfirmTotpSetupRequest");
}

#[test]
fn confirm_totp_setup_response_debug_redacts_backup_codes() {
    let r = ConfirmTotpSetupResponse {
        backup_codes: vec![
            "BACKUP_ONE_MUST_NOT_LEAK".to_string(),
            "BACKUP_TWO_MUST_NOT_LEAK".to_string(),
        ],
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(
        &rendered,
        &["BACKUP_ONE_MUST_NOT_LEAK", "BACKUP_TWO_MUST_NOT_LEAK"],
        "ConfirmTotpSetupResponse",
    );
    assert_redacted(&rendered, "ConfirmTotpSetupResponse");
    // The count is fine to expose (helps debugging), the values are not.
    assert!(rendered.contains('2'));
}

#[test]
fn enable_totp_response_debug_redacts_otpauth_uri() {
    let r = EnableTotpResponse {
        otpauth_uri: "otpauth://totp/PingVertex:x?secret=THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(&rendered, &["THIS_MUST_NOT_LEAK"], "EnableTotpResponse");
    assert_redacted(&rendered, "EnableTotpResponse");
}

#[test]
fn disable_totp_request_debug_redacts_password() {
    let r = DisableTotpRequest {
        password: "THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(&rendered, &["THIS_MUST_NOT_LEAK"], "DisableTotpRequest");
    assert_redacted(&rendered, "DisableTotpRequest");
}

#[test]
fn pending_two_factor_response_debug_redacts_pending_token() {
    let r = PendingTwoFactorResponse {
        pending_token: "pending_THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(
        &rendered,
        &["pending_THIS_MUST_NOT_LEAK"],
        "PendingTwoFactorResponse",
    );
    assert_redacted(&rendered, "PendingTwoFactorResponse");
}

#[test]
fn oauth_login_request_debug_redacts_id_token() {
    let r = OAuthLoginRequest {
        id_token: "oauth_THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{r:?}");
    assert_no_plaintext(
        &rendered,
        &["oauth_THIS_MUST_NOT_LEAK"],
        "OAuthLoginRequest",
    );
    assert_redacted(&rendered, "OAuthLoginRequest");
}
