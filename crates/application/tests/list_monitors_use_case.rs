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

    let create_use_case = CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new());

    let user_id = UserId(Uuid::new_v4());

    for i in 1..=3 {
        let request = CreateMonitorRequest {
            user_id,
            name: format!("Monitor {}", i),
            url: format!("https://example{}.com", i),
            method: HttpMethod::Get,
            expected_status: 200,
            interval_seconds: 60,
            timeout_seconds: 10,
            assertions: vec![],
            is_public: false,
            ssl_check_enabled: true,
            ssl_expiry_warning_days: 14,
        };

        create_use_case.execute(request).await.unwrap();
    }

    let list_use_case = ListMonitorsUseCase::new(repository);

    let monitors = list_use_case
        .execute(ListMonitors { user_id })
        .await
        .unwrap();

    assert_eq!(monitors.len(), 3);

    assert!(monitors.iter().any(|m| m.name.as_str() == "Monitor 1"));
    assert!(monitors.iter().any(|m| m.name.as_str() == "Monitor 2"));
    assert!(monitors.iter().any(|m| m.name.as_str() == "Monitor 3"));
}
