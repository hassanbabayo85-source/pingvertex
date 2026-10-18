//! TASK 2 — update_monitor rejects an interval below the tenant's
//! plan minimum via `PlanLimitError`, and permits the plan minimum.

mod common;
use common::*;

use std::sync::Arc;

use application::commands::UpdateMonitor;
use application::dto::CreateMonitorRequest;
use application::use_cases::{CreateMonitorUseCase, UpdateMonitorUseCase};
use domain::shared::ids::UserId;
use domain::{HttpMethod, PlanTier};
use infra::{InMemoryEntitlementResolver, InMemoryMonitorRepository, UuidIdGenerator};

use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────

fn create_request() -> CreateMonitorRequest {
    CreateMonitorRequest {
        regions: Vec::new(),
        name: "test-monitor".into(),
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 300,
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

fn update_command(id: domain::shared::ids::MonitorId, interval_seconds: u32) -> UpdateMonitor {
    UpdateMonitor {
        id,
        name: "test-monitor".into(),
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

// ── Tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn update_rejects_interval_below_free_minimum_with_plan_limit_error() {
    let repository = InMemoryMonitorRepository::new();
    let resolver = Arc::new(InMemoryEntitlementResolver::default());
    let user_id = UserId(Uuid::new_v4());
    let tenant = tenant_id();

    // Create a monitor at the Free minimum.
    let create_uc = CreateMonitorUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    )
    .with_entitlement_resolver(resolver.clone());

    let monitor = create_uc
        .execute(create_request(), auth_user(user_id, tenant))
        .await
        .unwrap();

    // Try to update to 299s → must fail with a structured PlanLimitError.
    let update_uc =
        UpdateMonitorUseCase::new(repository.clone(), new_audit_repo())
            .with_entitlement_resolver(resolver.clone());

    let err = update_uc
        .execute(update_command(monitor.id, 299), auth_user(user_id, tenant))
        .await
        .unwrap_err();

    let pl = err
        .downcast_ref::<application::PlanLimitError>()
        .expect("expected application::PlanLimitError, got a different error type");
    assert_eq!(pl.quota, "min_uptime_interval_seconds");
    assert_eq!(pl.current, 299);
    assert_eq!(pl.allowed, 300);
    assert_eq!(pl.action, "upgrade_plan");
    assert_eq!(pl.code(), "plan_limit_reached");
}

#[tokio::test]
async fn update_permits_exactly_the_plan_minimum() {
    let repository = InMemoryMonitorRepository::new();
    let resolver = Arc::new(InMemoryEntitlementResolver::default());
    let user_id = UserId(Uuid::new_v4());
    let tenant = tenant_id();

    let create_uc = CreateMonitorUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    )
    .with_entitlement_resolver(resolver.clone());

    let monitor = create_uc
        .execute(create_request(), auth_user(user_id, tenant))
        .await
        .unwrap();

    let update_uc =
        UpdateMonitorUseCase::new(repository, new_audit_repo())
            .with_entitlement_resolver(resolver);

    // 300s == plan minimum → should succeed.
    let updated = update_uc
        .execute(update_command(monitor.id, 300), auth_user(user_id, tenant))
        .await
        .unwrap();

    let updated = updated.expect("monitor must exist");
    assert_eq!(updated.interval_seconds, 300);
}

#[tokio::test]
async fn starter_plan_permits_60s_and_rejects_59s() {
    let repository = InMemoryMonitorRepository::new();
    let tenant = tenant_id();
    let starter = domain::Entitlements {
        plan_tier: PlanTier::Starter,
        entitlement_version: 1,
        max_uptime_monitors: 30,
        min_uptime_interval_seconds: 60,
        max_regions_per_monitor: 2,
        max_heartbeat_monitors: 10,
        max_api_flow_monitors: 2,
        max_api_flow_steps: 5,
        max_proxy_entries_per_job: 500,
        max_concurrent_proxy_jobs: 1,
        max_recurring_proxy_monitors: 0,
        min_recurring_proxy_interval_seconds: 300,
        max_recurring_proxy_executions_per_month: 0,
        max_rum_sites: 1,
        max_rum_events_per_month: 25_000,
    };
    let resolver =
        Arc::new(InMemoryEntitlementResolver::default().with_override(tenant, starter));
    let user_id = UserId(Uuid::new_v4());

    let create_uc = CreateMonitorUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    )
    .with_entitlement_resolver(resolver.clone());
    let monitor = create_uc
        .execute(create_request(), auth_user(user_id, tenant))
        .await
        .unwrap();

    let update_uc =
        UpdateMonitorUseCase::new(repository, new_audit_repo())
            .with_entitlement_resolver(resolver);

    // 60s → allowed.
    update_uc
        .execute(update_command(monitor.id, 60), auth_user(user_id, tenant))
        .await
        .unwrap()
        .expect("monitor must exist");

    // 59s → rejected.
    let err = update_uc
        .execute(update_command(monitor.id, 59), auth_user(user_id, tenant))
        .await
        .unwrap_err();
    let pl = err
        .downcast_ref::<application::PlanLimitError>()
        .expect("expected PlanLimitError");
    assert_eq!(pl.quota, "min_uptime_interval_seconds");
    assert_eq!(pl.allowed, 60);
    assert_eq!(pl.current, 59);
}

#[tokio::test]
async fn update_without_resolver_fails_closed() {
    let repository = InMemoryMonitorRepository::new();
    let resolver = Arc::new(InMemoryEntitlementResolver::default());
    let user_id = UserId(Uuid::new_v4());
    let tenant = tenant_id();

    let create_uc = CreateMonitorUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    )
    .with_entitlement_resolver(resolver);
    let monitor = create_uc
        .execute(create_request(), auth_user(user_id, tenant))
        .await
        .unwrap();

    // No `.with_entitlement_resolver(...)` on the update use case.
    let update_uc = UpdateMonitorUseCase::new(repository, new_audit_repo());
    let err = update_uc
        .execute(update_command(monitor.id, 300), auth_user(user_id, tenant))
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("entitlement resolver is not configured"),
        "got: {msg}"
    );
}
