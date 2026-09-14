//! PART 7 — Debug redaction regression tests (domain layer).
//!
//! Every credential-bearing domain entity/value object must render as
//! `[REDACTED]` in `Debug` output. If any of these tests fails, an
//! accidental `tracing::info!(?user)` (or any `{:?}`) would print
//! password hashes, TOTP secrets, webhook signing secrets, API-key
//! hashes, or token hashes in cleartext.

use chrono::{Duration, Utc};
use domain::{
    ApiKey, ApiKeyId, BackupCode, BackupCodeId, CreateApiKey, CreateWebhookChannel, Email,
    EmailVerificationToken, EmailVerificationTokenId, NotificationChannel, NotificationChannelId,
    PasswordHash, ResetPasswordToken, ResetPasswordTokenId, TenantId, UserBuilder, UserId,
    UserRole, UserStatus, Username,
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

// ===========================================================================
// User
// ===========================================================================

fn sample_user() -> domain::User {
    let now = Utc::now();
    UserBuilder::new()
        .id(UserId::new())
        .username(Username::new("part7").unwrap())
        .email(Email::new("part7@example.com").unwrap())
        .password_hash(PasswordHash::new("THIS_IS_THE_PASSWORD_HASH_MUST_NOT_LEAK").unwrap())
        .role(UserRole::User)
        .status(UserStatus::Active)
        .email_verified_at(Some(now))
        .created_at(now)
        .updated_at(now)
        .build()
        .unwrap()
}

#[test]
fn user_debug_redacts_password_hash_and_totp_secret() {
    let mut user = sample_user();
    user.enable_totp(vec![0xDE, 0xAD, 0xBE, 0xEF], Utc::now());

    let rendered = format!("{user:?}");

    assert_no_plaintext(
        &rendered,
        &[
            "THIS_IS_THE_PASSWORD_HASH_MUST_NOT_LEAK",
            // bytes rendered as decimal (if raw Vec<u8> were Debug-printed)
            "222, 173, 190, 239",
            // bytes rendered as hex (unlikely but possible)
            "deadbeef",
        ],
        "User::Debug",
    );
    assert_redacted(&rendered, "User::Debug");
    // Non-secret fields must still be visible for debugging.
    assert!(rendered.contains("part7"));
}

// ===========================================================================
// NotificationChannel
// ===========================================================================

#[test]
fn notification_channel_debug_redacts_signing_and_encrypted_secrets() {
    let channel = NotificationChannel::create_webhook(CreateWebhookChannel {
        id: NotificationChannelId(uuid::Uuid::new_v4()),
        monitor_id: domain::MonitorId(uuid::Uuid::new_v4()),
        user_id: UserId(uuid::Uuid::new_v4()),
        tenant_id: TenantId(uuid::Uuid::new_v4()),
        url: domain::Url::new("https://hooks.example.com/abc").unwrap(),
        signing_secret: "whsec_THIS_MUST_NOT_LEAK".to_string(),
    });

    let rendered = format!("{channel:?}");

    assert_no_plaintext(
        &rendered,
        &["whsec_THIS_MUST_NOT_LEAK"],
        "NotificationChannel::Debug (signing_secret)",
    );
    assert_redacted(&rendered, "NotificationChannel::Debug");
    // `target` is ALSO redacted on purpose: for Slack/Discord channels,
    // the webhook URL IS the credential; for webhooks it can carry a
    // query-string token.
    assert_no_plaintext(
        &rendered,
        &["https://hooks.example.com/abc"],
        "NotificationChannel::Debug (target)",
    );
}

// ===========================================================================
// ApiKey
// ===========================================================================

#[test]
fn api_key_debug_redacts_hashed_key() {
    let key = ApiKey::create(CreateApiKey {
        id: ApiKeyId::new(),
        tenant_id: TenantId::new(),
        user_id: UserId::new(),
        name: "part7-api-key".to_string(),
        key_prefix: "pv_part7".to_string(),
        hashed_key: "argon2_THIS_MUST_NOT_LEAK".to_string(),
        scopes: vec![],
        expires_at: None,
    });

    let rendered = format!("{key:?}");
    assert_no_plaintext(&rendered, &["argon2_THIS_MUST_NOT_LEAK"], "ApiKey::Debug");
    assert_redacted(&rendered, "ApiKey::Debug");
    assert!(rendered.contains("part7-api-key"));
}

// ===========================================================================
// ResetPasswordToken
// ===========================================================================

#[test]
fn reset_password_token_debug_redacts_prefix_and_hash() {
    let token = ResetPasswordToken::new(
        ResetPasswordTokenId::new(),
        UserId::new(),
        "rst_prefix_THIS_MUST_NOT_LEAK".to_string(),
        "hash_THIS_MUST_NOT_LEAK".to_string(),
        Utc::now() + Duration::minutes(30),
        Utc::now(),
    );

    let rendered = format!("{token:?}");
    assert_no_plaintext(
        &rendered,
        &["rst_prefix_THIS_MUST_NOT_LEAK", "hash_THIS_MUST_NOT_LEAK"],
        "ResetPasswordToken::Debug",
    );
    assert_redacted(&rendered, "ResetPasswordToken::Debug");
}

// ===========================================================================
// EmailVerificationToken
// ===========================================================================

#[test]
fn email_verification_token_debug_redacts_prefix_and_hash() {
    let token = EmailVerificationToken::new(
        EmailVerificationTokenId::new(),
        UserId::new(),
        "evt_prefix_THIS_MUST_NOT_LEAK".to_string(),
        "hash_THIS_MUST_NOT_LEAK".to_string(),
        Utc::now() + Duration::hours(1),
        Utc::now(),
    );

    let rendered = format!("{token:?}");
    assert_no_plaintext(
        &rendered,
        &["evt_prefix_THIS_MUST_NOT_LEAK", "hash_THIS_MUST_NOT_LEAK"],
        "EmailVerificationToken::Debug",
    );
    assert_redacted(&rendered, "EmailVerificationToken::Debug");
}

// ===========================================================================
// BackupCode
// ===========================================================================

#[test]
fn backup_code_debug_redacts_code_hash() {
    let code = BackupCode::new(
        BackupCodeId::new(),
        UserId::new(),
        "argon2_code_hash_THIS_MUST_NOT_LEAK".to_string(),
        Utc::now(),
    );

    let rendered = format!("{code:?}");
    assert_no_plaintext(
        &rendered,
        &["argon2_code_hash_THIS_MUST_NOT_LEAK"],
        "BackupCode::Debug",
    );
    assert_redacted(&rendered, "BackupCode::Debug");
}

// ===========================================================================
// PasswordHash
// ===========================================================================

#[test]
fn password_hash_debug_redacts_the_inner_hash() {
    let hash = PasswordHash::new("$argon2id$THIS_MUST_NOT_LEAK").unwrap();
    let rendered = format!("{hash:?}");
    assert_no_plaintext(
        &rendered,
        &["$argon2id$THIS_MUST_NOT_LEAK"],
        "PasswordHash::Debug",
    );
    assert_redacted(&rendered, "PasswordHash::Debug");
}

// ===========================================================================
// CreateApiKey / CreateWebhookChannel (the create-DTOs must be safe too)
// ===========================================================================

#[test]
fn create_api_key_debug_redacts_hashed_key() {
    let dto = CreateApiKey {
        id: ApiKeyId::new(),
        tenant_id: TenantId::new(),
        user_id: UserId::new(),
        name: "part7-create".to_string(),
        key_prefix: "pv_part7".to_string(),
        hashed_key: "argon2_THIS_MUST_NOT_LEAK".to_string(),
        scopes: vec![],
        expires_at: None,
    };
    let rendered = format!("{dto:?}");
    assert_no_plaintext(
        &rendered,
        &["argon2_THIS_MUST_NOT_LEAK"],
        "CreateApiKey::Debug",
    );
    assert_redacted(&rendered, "CreateApiKey::Debug");
}

#[test]
fn create_webhook_channel_debug_redacts_signing_secret() {
    let dto = CreateWebhookChannel {
        id: NotificationChannelId(uuid::Uuid::new_v4()),
        monitor_id: domain::MonitorId(uuid::Uuid::new_v4()),
        user_id: UserId(uuid::Uuid::new_v4()),
        tenant_id: TenantId(uuid::Uuid::new_v4()),
        url: domain::Url::new("https://hooks.example.com/abc").unwrap(),
        signing_secret: "whsec_THIS_MUST_NOT_LEAK".to_string(),
    };
    let rendered = format!("{dto:?}");
    assert_no_plaintext(
        &rendered,
        &["whsec_THIS_MUST_NOT_LEAK"],
        "CreateWebhookChannel::Debug",
    );
    assert_redacted(&rendered, "CreateWebhookChannel::Debug");
}
