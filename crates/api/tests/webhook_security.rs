use api::handlers::billing_webhook::{sanitize_financial_metadata, verify_signature};
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn webhook_signature_rejects_truncation_and_wrong_secret() {
    let body = br#"{"event_id":"e1"}"#;
    let valid = sign("secret", body);

    let mut headers = HeaderMap::new();
    headers.insert("x-billing-signature", valid.parse().unwrap());
    assert!(verify_signature("secret", &headers, body).is_ok());

    headers.insert("x-billing-signature", "sha256=00".parse().unwrap());
    assert!(verify_signature("secret", &headers, body).is_err());

    headers.insert("x-billing-signature", sign("wrong", body).parse().unwrap());
    assert!(verify_signature("secret", &headers, body).is_err());
}

#[test]
fn financial_metadata_never_retains_secret_like_keys() {
    let value = serde_json::json!({
        "password": "secret",
        "proxy_credentials": "user:pass",
        "api_token": "token",
        "nested": {"accessToken": "x", "safe": "ok"},
        "safe": "value"
    });
    let out = sanitize_financial_metadata(&value);
    assert_eq!(out["safe"], "value");
    assert!(out.get("password").is_none());
    assert!(out.get("proxy_credentials").is_none());
    assert!(out.get("api_token").is_none());
    assert!(out["nested"].get("accessToken").is_none());
}

#[test]
fn malformed_signature_is_always_rejected() {
    let body = b"{}";
    for signature in ["sha256=", "sha256=zz", "sha256=123", "hmac=deadbeef"] {
        let mut headers = HeaderMap::new();
        headers.insert("x-billing-signature", signature.parse().unwrap());
        assert!(
            verify_signature("secret", &headers, body).is_err(),
            "{signature}"
        );
    }
}
