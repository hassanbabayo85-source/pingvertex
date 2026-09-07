mod common;
use common::*;

use application::dto::CreateMonitorRequest;
use application::use_cases::{CreateMonitorError, CreateMonitorUseCase};
use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemorySubscriptionRepository, UuidIdGenerator};
use std::sync::Arc;

use uuid::Uuid;

#[tokio::test]
async fn create_monitor_persists_monitor() {
    let repository = InMemoryMonitorRepository::new();
    let use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());

    let request = CreateMonitorRequest {
        name: "Google".into(),
        url: "https://google.com".into(),
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
    };

    let monitor = use_case
        .execute(request, auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    assert_eq!(monitor.name.as_str(), "Google");
}

#[tokio::test]
async fn free_tier_enforces_300_second_minimum_and_20_monitor_quota() {
    let repository = InMemoryMonitorRepository::new();
    let subscriptions = Arc::new(InMemorySubscriptionRepository::new());
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new(), new_audit_repo())
        .with_entitlements(subscriptions);
    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let request = |name: String, interval_seconds: u32| CreateMonitorRequest {
        name,
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
    };

    let below_minimum = use_case
        .execute(
            request("free-299".into(), 299),
            auth_user(user_id, tenant_id),
        )
        .await;
    assert!(matches!(
        below_minimum,
        Err(CreateMonitorError::IntervalBelowPlanMinimum { min: 300 })
    ));

    for index in 0..20 {
        use_case
            .execute(
                request(format!("free-{index}"), 300),
                auth_user(user_id, tenant_id),
            )
            .await
            .unwrap();
    }

    let over_quota = use_case
        .execute(
            request("free-21".into(), 300),
            auth_user(user_id, tenant_id),
        )
        .await;
    assert!(matches!(
        over_quota,
        Err(CreateMonitorError::QuotaExceeded { limit: 20 })
    ));
}
