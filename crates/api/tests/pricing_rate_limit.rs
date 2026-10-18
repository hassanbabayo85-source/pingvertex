//! E2E coverage for the production billing rate limiter.
//!
//! The billing router is the current PAYG billing surface. This test uses the
//! real GovernorLayer configuration from router.rs rather than the old,
//! retired /pricing/quote endpoint.

use api::{auth_state::AuthState, router::build_billing_only_router_with_rate_limit, state::AppState};
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository,
    InMemoryMonitorIntervalPolicyRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryPAYGPricingRepository,
    InMemoryProxyCheckJobRepository, InMemoryRegistrationTransaction,
    InMemoryTenantMembershipRepository, InMemoryTenantRepository, NoopNotificationService,
    UuidIdGenerator,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

mod common;

const BURST_SIZE: usize = 10;

async fn build_app() -> (axum::Router, domain::UserId) {
    let monitor_repository = InMemoryMonitorRepository::new();

    let payg_repository = Arc::new(InMemoryPAYGPricingRepository::new());

    let interval_policies = InMemoryMonitorIntervalPolicyRepository::default()
        .with_policy(domain::MonitorKind::Http, 30)
        .with_policy(domain::MonitorKind::Tcp, 30)
        .with_policy(domain::MonitorKind::Dns, 30)
        .with_policy(domain::MonitorKind::Heartbeat, 30)
        .with_policy(domain::MonitorKind::Flow, 30);

    let tenant_id = domain::TenantId::new();
    let user_id = domain::UserId::new();

    application::ports::PAYGPricingRepository::create_version(
        &*payg_repository,
        "http_check.monitoring",
        "check",
        2,
        "USD",
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await
    .expect("test PAYG pricing must be created");

    let tenant_repository = InMemoryTenantRepository::new();
    tenant_repository.seed_with_id(tenant_id, user_id);

    let memberships = InMemoryTenantMembershipRepository::new();
    memberships.seed_owner(tenant_id, user_id);

    let app_state = AppState::new(
        monitor_repository.clone(),
        common::NoopCheckRepository,
        common::NoopDiagnosticReportRepository,
        common::NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repository),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(common::NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_payg_pricing_repository(payg_repository)
    .with_monitor_interval_policy_repository(interval_policies)
    .with_platform_admin_repositories(common::NoopUserRepository, tenant_repository.clone());

    let auth_state = AuthState::new(
        common::NoopUserRepository,
        common::NoopPasswordHasher,
        common::NoopTokenService,
        UuidIdGenerator::new(),
        tenant_repository,
        memberships,
        InMemoryRegistrationTransaction::new(),
    );

    (
        build_billing_only_router_with_rate_limit(app_state, auth_state),
        user_id,
    )
}

fn payg_preview_request(user_id: domain::UserId) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/payg/preview")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", user_id.0))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "monitors": 10,
                "interval_seconds": 30,
                "regions": 1,
                "forecast_days": 30,
                "kind": "Http"
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
async fn billing_requests_within_burst_are_allowed() {
    let (app, user_id) = build_app().await;

    for attempt in 0..BURST_SIZE {
        let response = app
            .clone()
            .oneshot(payg_preview_request(user_id))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} within the burst limit should succeed"
        );
    }
}

#[tokio::test]
async fn billing_requests_over_burst_are_rate_limited() {
    let (app, user_id) = build_app().await;

    for attempt in 0..BURST_SIZE {
        let response = app
            .clone()
            .oneshot(payg_preview_request(user_id))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} within the burst limit should succeed"
        );
    }

    let response = app
        .clone()
        .oneshot(payg_preview_request(user_id))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "request beyond the billing burst limit must be rejected with 429"
    );
}

#[tokio::test]
async fn billing_rate_limit_is_scoped_per_peer() {
    let (app, user_id) = build_app().await;

    for _ in 0..BURST_SIZE {
        let response = app
            .clone()
            .oneshot(payg_preview_request(user_id))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    let exhausted = app
        .clone()
        .oneshot(payg_preview_request(user_id))
        .await
        .unwrap();

    assert_eq!(exhausted.status(), StatusCode::TOO_MANY_REQUESTS);

    let mut other_peer = payg_preview_request(user_id);
    other_peer
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 42], 0))));

    let response = app.oneshot(other_peer).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a different peer must receive its own rate-limit bucket"
    );
}
