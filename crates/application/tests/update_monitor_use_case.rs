mod common;
use common::*;

use application::{
    commands::UpdateMonitor,
    dto::CreateMonitorRequest,
    use_cases::{CreateMonitorUseCase, UpdateMonitorUseCase},
};
use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn updates_monitor_successfully() {
    let repository = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let request = CreateMonitorRequest {
        regions: Vec::new(),
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

    let created = create_use_case
        .execute(request, auth_user(user_id, tenant_id))
        .await
        .unwrap();

    let update_use_case = UpdateMonitorUseCase::new(repository.clone(), new_audit_repo())
        .with_pricing_config_repository(std::sync::Arc::new(
            infra::InMemoryPricingConfigRepository::new()
                .with_config(domain::pricing::PricingConfig::default_config()),
        ));

    let updated = update_use_case
        .execute(
            UpdateMonitor {
                id: created.id,
                name: "Google Search".into(),
                url: "https://www.google.com".into(),
                method: HttpMethod::Head,
                expected_status: 200,
                interval_seconds: 300,
                timeout_seconds: 15,
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
            },
            auth_user(user_id, tenant_id),
        )
        .await
        .unwrap()
        .expect("monitor should exist");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name.as_str(), "Google Search");
    assert_eq!(updated.url, "https://www.google.com/");
}

#[tokio::test]
async fn cannot_update_monitor_owned_by_another_user() {
    let repository = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let request = CreateMonitorRequest {
        regions: Vec::new(),
        name: "Private Monitor".into(),
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
    };

    let created = create_use_case
        .execute(request, auth_user(owner_id, tenant_id()))
        .await
        .unwrap();

    let update_use_case = UpdateMonitorUseCase::new(repository.clone(), new_audit_repo())
        .with_pricing_config_repository(std::sync::Arc::new(
            infra::InMemoryPricingConfigRepository::new()
                .with_config(domain::pricing::PricingConfig::default_config()),
        ));
    let result = update_use_case
        .execute(
            UpdateMonitor {
                id: created.id,
                name: "Hacked Monitor".into(),
                url: "https://attacker.example.com".into(),
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
            },
            auth_user(attacker_id, tenant_id()),
        )
        .await
        .unwrap();

    assert!(result.is_none());
}

#[test]
fn enable_use_case_delegates_scheduler_state_to_repository() {
    let source = include_str!("../src/use_cases/update_monitor_enabled.rs");
    assert!(source.contains("set_enabled_for_tenant"));
    assert!(!source.contains("self.repository.save(&monitor)"));
}
