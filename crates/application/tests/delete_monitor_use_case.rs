mod common;
use common::*;

use application::{
    commands::DeleteMonitor,
    dto::CreateMonitorRequest,
    queries::GetMonitor,
    use_cases::{CreateMonitorUseCase, DeleteMonitorUseCase, GetMonitorUseCase},
};
use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn deletes_monitor_successfully() {
    let repository = InMemoryMonitorRepository::new();
    let create_use_case =
        CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new(), new_audit_repo());
    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let request = CreateMonitorRequest {
        name: "Google".into(),
        url: "https://google.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 60,
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

    let monitor = create_use_case
        .execute(request, auth_user(user_id, tenant_id))
        .await
        .unwrap();

    let delete_use_case = DeleteMonitorUseCase::new(repository.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(
            DeleteMonitor { id: monitor.id },
            auth_user(user_id, tenant_id),
        )
        .await
        .unwrap();
    assert!(deleted);

    let get_use_case = GetMonitorUseCase::new(repository);
    let result = get_use_case
        .execute(GetMonitor { id: monitor.id }, auth_user(user_id, tenant_id))
        .await
        .unwrap();
    assert!(result.is_none());
}
