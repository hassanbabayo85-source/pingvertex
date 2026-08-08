use application::{
    dto::CreateMonitorRequest,
    queries::GetMonitor,
    use_cases::{CreateMonitorUseCase, GetMonitorUseCase},
};

use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn gets_monitor_by_id_successfully() {
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

    let created = create_use_case.execute(request).await.unwrap();

    let get_use_case = GetMonitorUseCase::new(repository);

    let monitor = get_use_case
        .execute(GetMonitor {
            id: created.id,
            user_id,
        })
        .await
        .unwrap()
        .expect("monitor should exist");

    assert_eq!(monitor.id, created.id);
    assert_eq!(monitor.name.as_str(), "Google");
}
