use std::sync::OnceLock;

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::warn;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Prefix of the standard `Authorization: Bearer <token>` header, which a
/// Prometheus scrape_config can send directly via
/// `authorization: { type: Bearer, credentials_file: ... }` — no custom
/// header required.
const BEARER_PREFIX: &str = "Bearer ";

pub fn init_metrics() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");

    let _ = PROMETHEUS_HANDLE.set(handle);
}

pub fn render_metrics() -> String {
    PROMETHEUS_HANDLE
        .get()
        .map(|handle| handle.render())
        .unwrap_or_default()
}

/// Compares two &str values in constant time, to close the timing
/// side-channel that would otherwise let an attacker recover
/// METRICS_TOKEN character by character.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());

    if a.len() != b.len() {
        return false;
    }

    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }

    diff == 0
}

/// Protected `/metrics` handler.
///
/// FAIL-CLOSED: if `METRICS_TOKEN` is not set in the environment, the
/// endpoint returns 404 (without revealing that it exists) rather than
/// serving metrics to anyone — the 2026-08-11 security audit found that
/// metrics were unprotected, which exposed monitor counts, latency, and
/// other internal operational data to anyone who could reach the endpoint.
///
/// If `METRICS_TOKEN` is set, the caller must send exactly that value in an
/// `Authorization: Bearer <token>` header; otherwise the response is
/// 401.
pub async fn metrics_handler(headers: HeaderMap) -> Response {
    let expected_token = match std::env::var("METRICS_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ => {
            warn!(
                "METRICS_TOKEN is not set in the environment — the /metrics endpoint is \
                 closed (fail-closed). Set METRICS_TOKEN to allow Prometheus \
                 scraping."
            );
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let provided_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
        .unwrap_or("");

    if !constant_time_eq(provided_token, &expected_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    render_metrics().into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("my-secret-token", "my-secret-token"));
    }

    #[test]
    fn constant_time_eq_rejects_different_strings() {
        assert!(!constant_time_eq("my-secret-token", "wrong-token"));
    }

    #[test]
    fn constant_time_eq_rejects_different_length_strings() {
        assert!(!constant_time_eq("short", "much-longer-string"));
    }

    #[test]
    fn constant_time_eq_rejects_empty_provided_token() {
        assert!(!constant_time_eq("", "my-secret-token"));
    }

    #[test]
    fn constant_time_eq_treats_empty_equal_to_empty() {
        // This only exercises the comparison function itself; the handler
        // never calls it when METRICS_TOKEN is unset (it returns a
        // fail-closed 404 first), so there is no risk here.
        assert!(constant_time_eq("", ""));
    }

    // NOTE: these tests mutate the `METRICS_TOKEN` env var, so they are all
    // wrapped in a hand-rolled `#[serial]`-style pattern (one test, run
    // sequentially) to prevent a race between tests running concurrently. If
    // more env-var-mutating tests are added later, fold them into this single
    // function or switch to the `serial_test` crate.
    #[tokio::test]
    async fn metrics_handler_env_var_scenarios() {
        use axum::body::to_bytes;
        use axum::http::{HeaderValue, header::AUTHORIZATION};

        // 1) Babu METRICS_TOKEN a environment => fail-closed 404.
        unsafe {
            std::env::remove_var("METRICS_TOKEN");
        }
        let response = metrics_handler(HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // 2) METRICS_TOKEN is set but there is no Authorization header => 401.
        unsafe {
            std::env::set_var("METRICS_TOKEN", "test-token-abc123");
        }
        let response = metrics_handler(HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 3) An turo token da bai dace ba => 401.
        let mut wrong_headers = HeaderMap::new();
        wrong_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        let response = metrics_handler(wrong_headers).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 4) An turo daidai token din => 200, tare da metrics body.
        let mut correct_headers = HeaderMap::new();
        correct_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-abc123"),
        );
        let response = metrics_handler(correct_headers).await;
        assert_eq!(response.status(), StatusCode::OK);
        // Reaching here means the auth check passed correctly. No recorder
        // is installed in this test binary, so we only assert that reading
        // the body does not panic or error.
        let _body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        unsafe {
            std::env::remove_var("METRICS_TOKEN");
        }
    }
}
