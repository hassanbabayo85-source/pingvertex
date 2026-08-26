//! E2E test for Worker /metrics Bearer authentication (TASK A).
//!
//! Starts real HTTP servers on localhost with the same router as
//! production, then verifies:
//! - without token configured -> 404 (fail-closed)
//! - wrong token -> 401
//! - correct token -> 200
//! - health endpoint remains open

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::http::header::AUTHORIZATION;
use metrics as prometheus_metrics;

const TEST_PORT: u16 = 3999;

#[tokio::test]
async fn worker_metrics_endpoint_requires_bearer_token() {
    // Install the Prometheus recorder and register a test metric so the
    // /metrics body is non-empty when the correct token is provided.
    worker::metrics::init_metrics();
    prometheus_metrics::counter!("worker_e2e_test_metric").increment(1);

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), TEST_PORT);

    // Part 1: no token configured -> /metrics should be 404.
    {
        let server = tokio::spawn(async move {
            let _ = worker::metrics::serve_metrics_on_with_token(addr, None).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{}/metrics", addr))
            .send()
            .await
            .expect("request without token");
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

        server.abort();
    }

    // Give the port a moment to be released.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Part 2: token configured -> wrong token 401, correct 200.
    let expected_token = "e2e-worker-metrics-test-token".to_string();
    let token_for_server = expected_token.clone();
    let server = tokio::spawn(async move {
        let _ = worker::metrics::serve_metrics_on_with_token(addr, Some(token_for_server)).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Wrong token -> 401.
    let resp = client
        .get(format!("http://{}/metrics", addr))
        .header(AUTHORIZATION, "Bearer wrong-token")
        .send()
        .await
        .expect("request with wrong token");
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Correct token -> 200.
    let resp = client
        .get(format!("http://{}/metrics", addr))
        .header(AUTHORIZATION, format!("Bearer {expected_token}"))
        .send()
        .await
        .expect("request with correct token");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.expect("metrics body");
    assert!(!body.is_empty());

    // Health endpoint remains unauthenticated (liveness).
    let resp = client
        .get(format!("http://{}/health", addr))
        .send()
        .await
        .expect("health request");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    server.abort();
}
