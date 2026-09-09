mod common;
use common::*;

use application::{
    dto::CreateMonitorRequest,
    queries::ListMonitors,
    use_cases::{CreateMonitorUseCase, ListMonitorsUseCase},
};
use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn lists_all_monitors_successfully() {
    let repository = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    for i in 1..=3 {
        let request = CreateMonitorRequest {
            regions: Vec::new(),
            name: format!("Monitor {}", i),
            url: format!("https://example{}.com", i),
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

        create_use_case
            .execute(request, auth_user(user_id, tenant_id))
            .await
            .unwrap();
    }

    let list_use_case = ListMonitorsUseCase::new(repository);
    let monitors = list_use_case
        .execute(ListMonitors, auth_user(user_id, tenant_id))
        .await
        .unwrap();
    assert_eq!(monitors.len(), 3);
}
