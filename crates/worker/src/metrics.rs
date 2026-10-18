use std::{error::Error, net::SocketAddr, sync::OnceLock};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::{info, warn};

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

const BEARER_PREFIX: &str = "Bearer ";

pub fn init_metrics() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder for worker");

    let _ = PROMETHEUS_HANDLE.set(handle);
}

pub fn render_metrics() -> String {
    PROMETHEUS_HANDLE
        .get()
        .map(|handle| handle.render())
        .unwrap_or_default()
}

/// Constant-time comparison to avoid timing side-channels.
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

/// Handler na `/metrics` mai kariya da `METRICS_TOKEN`.
///
/// `expected_token` comes from Axum `State`. A `None` or empty token
/// means fail-closed: 404, the same as when the env var is unset.
pub async fn metrics_handler(
    State(expected_token): State<Option<String>>,
    headers: HeaderMap,
) -> Response {
    let expected_token = match expected_token {
        Some(token) if !token.trim().is_empty() => token,
        _ => {
            warn!(
                "METRICS_TOKEN is not set in the environment — the /metrics endpoint is \
                 rufe (fail-closed). Saita METRICS_TOKEN domin bada damar Prometheus \
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

/// Liveness probe for the worker process, mirroring
/// `api::handlers::health::health` — zero-dependency, only proves
/// this HTTP server itself is up.
async fn health() -> StatusCode {
    StatusCode::OK
}

/// Build the metrics/health router. Token is held in state so tests can
/// pass a known value without relying on environment-variable races.
fn build_metrics_router(expected_token: Option<String>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health))
        .with_state(expected_token)
}

/// Serve metrics/health on the specified address, reading
/// `METRICS_TOKEN` from the environment.
pub async fn serve_metrics_on(addr: SocketAddr) -> Result<(), Box<dyn Error + Send + Sync>> {
    let expected_token = std::env::var("METRICS_TOKEN").ok();
    serve_metrics_on_with_token(addr, expected_token).await
}

/// Serve metrics/health on the specified address with an explicit token.
/// Used by tests to avoid environment-variable mutation races.
pub async fn serve_metrics_on_with_token(
    addr: SocketAddr,
    expected_token: Option<String>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let app = build_metrics_router(expected_token);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("Worker metrics/health listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve metrics/health on the default worker metrics port (3001).
pub async fn serve_metrics() -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    serve_metrics_on(addr).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

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

    #[tokio::test]
    async fn metrics_handler_env_var_scenarios() {
        use axum::http::{HeaderValue, header::AUTHORIZATION};

        // 1) Babu token => fail-closed 404.
        let response = metrics_handler(State(None), HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // 2) Token set, no Authorization header => 401.
        let response = metrics_handler(
            State(Some("test-token-abc123".to_string())),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 3) Wrong token => 401.
        let mut wrong_headers = HeaderMap::new();
        wrong_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        let response =
            metrics_handler(State(Some("test-token-abc123".to_string())), wrong_headers).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 4) Correct token => 200, with metrics body.
        let mut correct_headers = HeaderMap::new();
        correct_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-abc123"),
        );
        let response = metrics_handler(
            State(Some("test-token-abc123".to_string())),
            correct_headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let _body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    }
}
