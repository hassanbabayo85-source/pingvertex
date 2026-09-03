//! E2E coverage for H10: `pricing_routes` must carry a `GovernorLayer`
//! IP-based rate limiter, matching every other route group wired in
//! `router.rs::build_router`.
//!
//! This deliberately does NOT go through `build_test_router` (used by every
//! other E2E test in this suite, via `mod test_app;` or an equivalent local
//! `build_test_app()` helper) — `build_test_router` intentionally omits
//! `GovernorLayer` from every route so the rest of the E2E suite isn't
//! flaky/order-dependent under IP-based rate limiting. Instead this uses
//! `build_pricing_only_router_with_rate_limit`, a minimal router exposing
//! only the pricing routes with their real production `GovernorLayer`
//! attached — mirroring the existing, analogous
//! `build_auth_only_router_with_rate_limit`.
//!
//! `tower_governor`'s default `PeerIpKeyExtractor` reads
//! `ConnectInfo<SocketAddr>` from the request's extensions, which is
//! normally inserted by axum's connection layer
//! (`into_make_service_with_connect_info`) and is therefore absent under a
//! bare `oneshot()` call. Each request below inserts that extension by
//! hand so the limiter has a stable per-"IP" bucket to key on.

use api::{router::build_pricing_only_router_with_rate_limit, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, SecretCipher,
    SecretCipherError,
};
use async_trait::async_trait;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use domain::pricing::PricingConfig;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId};
use domain::{Check, DiagnosticReport, Incident};
use infra::repositories::in_memory_pricing_config_repository::InMemoryPricingConfigRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, UuidIdGenerator,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Minimal in-memory/noop repositories, same shape as the other E2E test
// files in this crate (e.g. `test_app.rs`, `heartbeat.rs`) — pricing_quote
// only actually reads `pricing_config_repository`, everything else here is
// unused plumbing `AppState::new` still requires.
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct NoopSecretCipher;

impl SecretCipher for NoopSecretCipher {
    fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(plaintext.to_vec())
    }
    fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(ciphertext.to_vec())
    }
}

#[derive(Clone, Default)]
struct NoopCheckRepository;

#[async_trait]
impl CheckRepository for NoopCheckRepository {
    async fn save(&self, _: &Check) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _: CheckId,
    ) -> Result<Option<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn list_checks(
        &self,
        _: u32,
        _: u32,
        _: TenantId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn delete_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        Ok(0)
    }
}

#[derive(Clone, Default)]
struct NoopDiagnosticReportRepository;

#[async_trait]
impl DiagnosticReportRepository for NoopDiagnosticReportRepository {
    async fn save(
        &self,
        _: &DiagnosticReport,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_check_id(
        &self,
        _: CheckId,
    ) -> Result<Option<DiagnosticReport>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct NoopIncidentRepository;

#[async_trait]
impl IncidentRepository for NoopIncidentRepository {
    async fn save(&self, _: &Incident) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn update(&self, _: &Incident) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn find_open_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

/// Must match the `GovernorConfigBuilder::default().per_second(2)
/// .burst_size(30)` wired onto `pricing_routes` in
/// `router.rs::build_router` (H10). If that production config changes,
/// this constant (and therefore this test) must change with it.
const BURST_SIZE: usize = 30;

fn build_app() -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();

    let pricing_repo = Arc::new(
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config()),
    );

    let app_state = AppState::new(
        monitor_repository.clone(),
        NoopCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repository),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_pricing_config_repository(pricing_repo);

    build_pricing_only_router_with_rate_limit(app_state)
}

/// Builds a `POST /api/v1/pricing/quote` request with a fixed fake peer
/// address inserted as `ConnectInfo<SocketAddr>`, so every call in a given
/// test lands in the same `PeerIpKeyExtractor` bucket (see module docs).
fn quote_request() -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/pricing/quote")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "monitors": 10,
                "interval_seconds": 60,
                "retention_days": 30
            }))
            .unwrap(),
        ))
        .unwrap();

    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));

    request
}

#[tokio::test]
async fn pricing_quote_requests_within_burst_are_allowed() {
    let app = build_app();

    for attempt in 0..BURST_SIZE {
        let response = app.clone().oneshot(quote_request()).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} within the burst limit should succeed"
        );
    }
}

#[tokio::test]
async fn pricing_quote_requests_over_burst_are_rate_limited() {
    let app = build_app();

    for attempt in 0..BURST_SIZE {
        let response = app.clone().oneshot(quote_request()).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} within the burst limit should succeed"
        );
    }

    let over_limit_response = app.clone().oneshot(quote_request()).await.unwrap();
    assert_eq!(
        over_limit_response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "request beyond the burst limit must be rejected with 429"
    );
}

#[tokio::test]
async fn pricing_quote_requests_from_a_different_peer_get_their_own_bucket() {
    // Sanity check that the limiter is genuinely keyed per-IP (not global):
    // exhausting one fake peer's bucket must not affect a different peer.
    let app = build_app();

    for _ in 0..BURST_SIZE {
        let response = app.clone().oneshot(quote_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let exhausted = app.clone().oneshot(quote_request()).await.unwrap();
    assert_eq!(exhausted.status(), StatusCode::TOO_MANY_REQUESTS);

    let mut other_peer_request = Request::builder()
        .method("POST")
        .uri("/api/v1/pricing/quote")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "monitors": 10,
                "interval_seconds": 60,
                "retention_days": 30
            }))
            .unwrap(),
        ))
        .unwrap();
    other_peer_request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 42], 0))));

    let other_peer_response = app.clone().oneshot(other_peer_request).await.unwrap();
    assert_eq!(other_peer_response.status(), StatusCode::OK);
}
