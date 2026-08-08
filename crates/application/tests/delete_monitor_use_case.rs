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

    let create_use_case = CreateMonitorUseCase::new(repository.clone(), UuidIdGenerator::new());

    let user_id = UserId(Uuid::new_v4());

    let request = CreateMonitorRequest {
        user_id,
        name: "Google".into(),
        url: "https://google.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 60,
        timeout_seconds: 10,
    };

    let monitor = create_use_case.execute(request).await.unwrap();

    let delete_use_case = DeleteMonitorUseCase::new(repository.clone());

    let deleted = delete_use_case
        .execute(DeleteMonitor {
            id: monitor.id,
            user_id,
        })
        .await
        .unwrap();

    assert!(deleted);

    let get_use_case = GetMonitorUseCase::new(repository);

    let result = get_use_case
        .execute(GetMonitor {
            id: monitor.id,
            user_id,
        })
        .await
        .unwrap();

    assert!(result.is_none());
}
