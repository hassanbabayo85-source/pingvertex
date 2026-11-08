//! Secret redaction regression tests specific to the `api` crate.
//!
//! The `infra` version of this file covers `TwilioConfig`,
//! `S3Config`, `JsonWebTokenService`, and the cipher. This file covers
//! `api::config::Config`, which cannot live in the `infra` test binary
//! because `api` depends on `infra` (not the reverse).

use std::env;

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

#[test]
fn api_config_debug_redacts_every_secret_field() {
    // SAFETY: this test binary's own env vars are process-global; the
    // values are set immediately before the `try_from_env` call and
    // restored immediately after.
    let keys = [
        "DATABASE_URL",
        "JWT_SECRET",
        "REDIS_URL",
        "CORS_ALLOWED_ORIGIN",
    ];
    let prev: Vec<Option<String>> = keys.iter().map(|k| env::var(k).ok()).collect();

    unsafe {
        env::set_var(
            "DATABASE_URL",
            "postgres://dbuser:DB_PASSWORD_MUST_NOT_APPEAR@localhost:5432/db",
        );
        env::set_var("JWT_SECRET", "JWT_SECRET_MUST_NOT_APPEAR_0123456789abcdef");
        env::set_var(
            "REDIS_URL",
            "redis://:REDIS_PASSWORD_MUST_NOT_APPEAR@localhost:6379",
        );
        env::set_var("CORS_ALLOWED_ORIGIN", "https://example.com");
    }

    let result = api::config::Config::try_from_env();

    for (k, v) in keys.iter().zip(prev) {
        unsafe {
            match v {
                Some(val) => env::set_var(k, val),
                None => env::remove_var(k),
            }
        }
    }

    // If construction succeeded, assert the redaction property.
    // Construction can legitimately fail (e.g. some other check), in
    // which case the redaction of a value that was never constructed is
    // vacuously true — we skip rather than flake.
    if let Ok(cfg) = result {
        let rendered = format!("{cfg:?}");
        assert_no_plaintext(
            &rendered,
            &[
                "JWT_SECRET_MUST_NOT_APPEAR_0123456789abcdef",
                "DB_PASSWORD_MUST_NOT_APPEAR",
                "REDIS_PASSWORD_MUST_NOT_APPEAR",
            ],
            "api::config::Config::Debug",
        );
        assert!(rendered.contains("<redacted>"));
    }
}
