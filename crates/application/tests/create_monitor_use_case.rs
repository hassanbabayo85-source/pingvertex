mod common;
use common::*;

use std::sync::Arc;

use application::dto::CreateMonitorRequest;
use application::use_cases::{CreateMonitorError, CreateMonitorUseCase};
use domain::shared::ids::UserId;
use domain::{HttpMethod, RegionCode};
use infra::{InMemoryEntitlementResolver, InMemoryMonitorRepository, UuidIdGenerator};

use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────

fn req(name: impl Into<String>, interval_seconds: u32) -> CreateMonitorRequest {
    CreateMonitorRequest {
        regions: Vec::new(),
        name: name.into(),
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    }
}

fn resolver_default() -> Arc<InMemoryEntitlementResolver> {
    Arc::new(InMemoryEntitlementResolver::default())
}

// ── Tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn create_monitor_persists_monitor() {
    let repository = InMemoryMonitorRepository::new();
    let use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo())
            .with_entitlement_resolver(resolver_default());

    let user_id = UserId(Uuid::new_v4());
    let monitor = use_case
        .execute(req("Google", 300), auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    assert_eq!(monitor.name.as_str(), "Google");
}

#[tokio::test]
async fn free_tier_rejects_interval_below_300_seconds_via_plan_limit_error() {
    let repository = InMemoryMonitorRepository::new();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_entitlement_resolver(resolver_default());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let err = use_case
        .execute(req("free-299", 299), auth_user(user_id, tenant_id))
        .await
        .unwrap_err();

    match err {
        CreateMonitorError::PlanLimitExceeded(pl) => {
            assert_eq!(pl.quota, "min_uptime_interval_seconds");
            assert_eq!(pl.allowed, 300);
            assert_eq!(pl.current, 299);
            assert_eq!(pl.action, "upgrade_plan");
            assert_eq!(pl.code(), "plan_limit_reached");
        }
        other => panic!("expected PlanLimitExceeded for interval, got {other:?}"),
    }
}

#[tokio::test]
async fn free_tier_permits_20_monitors_and_rejects_the_21st() {
    let repository = InMemoryMonitorRepository::new();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_entitlement_resolver(resolver_default());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    for i in 0..20 {
        use_case
            .execute(req(format!("free-{i}"), 300), auth_user(user_id, tenant_id))
            .await
            .unwrap_or_else(|e| panic!("create #{} should succeed: {e:?}", i));
    }

    let err = use_case
        .execute(req("free-21", 300), auth_user(user_id, tenant_id))
        .await
        .unwrap_err();

    match err {
        CreateMonitorError::PlanLimitExceeded(pl) => {
            assert_eq!(pl.quota, "max_uptime_monitors");
            assert_eq!(pl.allowed, 20);
            assert_eq!(pl.code(), "plan_limit_reached");
        }
        other => panic!("expected PlanLimitExceeded for count, got {other:?}"),
    }
}

#[tokio::test]
async fn starter_plan_allows_60_seconds_and_30_monitors() {
    let repository = InMemoryMonitorRepository::new();
    let tenant_id = tenant_id();
    let starter = domain::Entitlements::starter_v1_fixture();
    let resolver = Arc::new(
        InMemoryEntitlementResolver::default().with_override(tenant_id, starter),
    );
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_entitlement_resolver(resolver);

    let user_id = UserId(Uuid::new_v4());

    // 60s is allowed for Starter.
    use_case
        .execute(req("starter-1", 60), auth_user(user_id, tenant_id))
        .await
        .unwrap();

    // 59s is below the Starter floor.
    let err = use_case
        .execute(req("starter-2", 59), auth_user(user_id, tenant_id))
        .await
        .unwrap_err();
    match err {
        CreateMonitorError::PlanLimitExceeded(pl) => {
            assert_eq!(pl.quota, "min_uptime_interval_seconds");
            assert_eq!(pl.allowed, 60);
            assert_eq!(pl.current, 59);
        }
        other => panic!("expected PlanLimitExceeded for interval, got {other:?}"),
    }
}

#[tokio::test]
async fn free_tier_rejects_more_than_one_region() {
    let repository = InMemoryMonitorRepository::new();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_entitlement_resolver(resolver_default());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let mut request = req("multi-region", 300);
    request.regions = vec![
        RegionCode::new("us-east").unwrap(),
        RegionCode::new("eu-west").unwrap(),
    ];

    let err = use_case
        .execute(request, auth_user(user_id, tenant_id))
        .await
        .unwrap_err();
    match err {
        CreateMonitorError::PlanLimitExceeded(pl) => {
            assert_eq!(pl.quota, "max_regions_per_monitor");
            assert_eq!(pl.allowed, 1);
            assert_eq!(pl.current, 2);
        }
        other => panic!("expected PlanLimitExceeded for regions, got {other:?}"),
    }
}

#[tokio::test]
async fn missing_entitlement_resolver_fails_closed() {
    let repository = InMemoryMonitorRepository::new();
    // Note: NO `.with_entitlement_resolver(...)` call.
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo());

    let err = use_case
        .execute(req("no-resolver", 300), auth_user(UserId(Uuid::new_v4()), tenant_id()))
        .await
        .unwrap_err();

    matches!(err, CreateMonitorError::EntitlementResolverMissing);
}
