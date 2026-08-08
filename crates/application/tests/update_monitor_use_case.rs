use application::{
    commands::UpdateMonitor,
    dto::CreateMonitorRequest,
    queries::GetMonitor,
    use_cases::{CreateMonitorUseCase, GetMonitorUseCase, UpdateMonitorUseCase},
};

use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn updates_monitor_successfully() {
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

    let update_use_case = UpdateMonitorUseCase::new(repository.clone());

    let updated = update_use_case
        .execute(UpdateMonitor {
            id: created.id,
            user_id,
            name: "Google Search".into(),
            url: "https://www.google.com".into(),
            method: HttpMethod::Head,
            expected_status: 200,
            interval_seconds: 120,
            timeout_seconds: 15,
        })
        .await
        .unwrap()
        .expect("monitor should exist");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name.as_str(), "Google Search");
    assert_eq!(updated.url, "https://www.google.com/");

    let get_use_case = GetMonitorUseCase::new(repository);

    let fetched = get_use_case
        .execute(GetMonitor {
            id: created.id,
            user_id,
        })
        .await
        .unwrap()
        .expect("monitor should still exist");

    assert_eq!(fetched.name.as_str(), "Google Search");
    assert_eq!(fetched.method, HttpMethod::Head);
}
