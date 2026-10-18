use api::handlers::billing_webhook::{sanitize_financial_metadata, verify_signature};
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[test]
fn webhook_signature_is_constant_time_verified_and_rejects_wrong_length() {
    let body = b"{}";
    let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
    mac.update(body);
    let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    let mut h = HeaderMap::new();
    h.insert("x-billing-signature", sig.parse().unwrap());
    assert!(verify_signature("secret", &h, body).is_ok());
    h.insert("x-billing-signature", "sha256=00".parse().unwrap());
    assert!(verify_signature("secret", &h, body).is_err());
}

#[test]
fn financial_webhook_metadata_sanitization_contract_is_secret_free() {
    let value = serde_json::json!({"password":"x","proxy_credentials":"x","api_secret":"x","api_key":"x","safe":"ok"});
    let sanitized = sanitize_financial_metadata(&value);
    let text = sanitized.to_string();
    assert!(text.contains("safe"));
    assert!(!text.contains("password"));
    assert!(!text.contains("credential"));
    assert!(!text.contains("secret"));
    assert!(!text.contains("api_key"));
}
