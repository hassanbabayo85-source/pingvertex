//! Secret redaction regression tests.
//!
//! Every secret-bearing struct must render as `<redacted>` in `Debug`
//! output. If any of these tests fails, an accidental
//! `tracing::info!(?config, ...)` — or a `{:?}` in an error path — would
//! print key material in cleartext.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Asserts that `rendered` does NOT contain any of the plaintext needles.
fn assert_no_plaintext(rendered: &str, needles: &[&str], what: &str) {
    for needle in needles {
        assert!(
            !needle.is_empty(),
            "test bug: an empty needle would pass vacuously"
        );
        assert!(
            !rendered.contains(needle),
            "SECURITY: {what} leaked plaintext '{needle}' in Debug output: {rendered}"
        );
    }
}

// ---------------------------------------------------------------------------
// TwilioConfig
// ---------------------------------------------------------------------------

#[test]
fn twilio_config_debug_redacts_auth_token() {
    let cfg = infra::TwilioConfig {
        account_sid: "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
        auth_token: "AUTH_TOKEN_MUST_NOT_APPEAR".to_string(),
        from_number: "+15555550100".to_string(),
    };
    let rendered = format!("{cfg:?}");
    assert_no_plaintext(
        &rendered,
        &["AUTH_TOKEN_MUST_NOT_APPEAR"],
        "TwilioConfig::Debug",
    );
    assert!(rendered.contains("<redacted>"));
}

// ---------------------------------------------------------------------------
// S3Config
// ---------------------------------------------------------------------------

#[test]
fn s3_config_debug_redacts_both_credentials() {
    let cfg = infra::S3Config {
        bucket: "my-bucket".to_string(),
        region: "us-east-1".to_string(),
        endpoint: Some("https://s3.example.com".to_string()),
        access_key_id: "AKIA_MUST_NOT_APPEAR".to_string(),
        secret_access_key: "SECRET_KEY_MUST_NOT_APPEAR".to_string(),
        public_base_url: "https://cdn.example.com".to_string(),
        path_style: true,
    };
    let rendered = format!("{cfg:?}");
    assert_no_plaintext(
        &rendered,
        &["AKIA_MUST_NOT_APPEAR", "SECRET_KEY_MUST_NOT_APPEAR"],
        "S3Config::Debug",
    );
    assert!(rendered.contains("<redacted>"));
    assert!(rendered.contains("my-bucket"));
}

// ---------------------------------------------------------------------------
// JsonWebTokenService
// ---------------------------------------------------------------------------

#[test]
fn jsonwebtoken_service_debug_redacts_signing_secret() {
    let svc = infra::JsonWebTokenService::new("JWT_SECRET_MUST_NOT_APPEAR");
    let rendered = format!("{svc:?}");
    assert_no_plaintext(
        &rendered,
        &["JWT_SECRET_MUST_NOT_APPEAR"],
        "JsonWebTokenService::Debug",
    );
    assert!(rendered.contains("<redacted>"));
}

// ---------------------------------------------------------------------------
// AesGcmProxyCredentialCipher
// ---------------------------------------------------------------------------

#[test]
fn proxy_credential_cipher_debug_redacts_key() {
    // 32-byte key expressed in hex; if Debug leaked the key struct we'd
    // see it.
    let cipher = infra::AesGcmProxyCredentialCipher::new([0xAB; 32]);
    let rendered = format!("{cipher:?}");
    // The cipher's Debug impl is empty (`finish()`), so we assert only
    // the shape: no hex/byte fragments from the key.
    assert_no_plaintext(
        &rendered,
        &["ABAB", "abab", "171", "[171"], // 0xAB == 171 decimal
        "AesGcmProxyCredentialCipher::Debug",
    );
    // Sanity: the type name should still be present (it is a struct Debug).
    assert!(rendered.contains("AesGcmProxyCredentialCipher"));
}

// ---------------------------------------------------------------------------
// Config (api) — database_url, jwt_secret, redis_url, mtls_server_key_path

// ---------------------------------------------------------------------------
// Error types — no secret material in Display/Debug
// ---------------------------------------------------------------------------

#[test]
fn proxy_credential_key_error_never_carries_key_material() {
    use infra::services::ProxyCredentialEncryptionKeyError;
    let cases = [
        ProxyCredentialEncryptionKeyError::MissingKey,
        ProxyCredentialEncryptionKeyError::UndecodableKey,
        ProxyCredentialEncryptionKeyError::WrongKeyLength { decoded_len: 16 },
    ];
    for err in cases {
        let rendered = format!("{err:?} {err}");
        // The variant carries only lengths/format facts, never the raw
        // bytes. The rendering must be one of the known safe shapes and
        // never contain a 64-char hex string (a plausible leaked key).
        for token in rendered.split_whitespace() {
            assert!(
                token.len() != 64 || !token.chars().all(|c| c.is_ascii_hexdigit()),
                "SECURITY: ProxyCredentialEncryptionKeyError leaked a 64-char hex string: {token}"
            );
        }
    }
}
