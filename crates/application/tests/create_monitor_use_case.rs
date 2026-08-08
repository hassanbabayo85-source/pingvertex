use application::{dto::CreateMonitorRequest, use_cases::CreateMonitorUseCase};

use domain::HttpMethod;
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn creates_monitor_successfully() {
    let repository = InMemoryMonitorRepository::new();
    let use_case = CreateMonitorUseCase::new(repository, UuidIdGenerator::new());

    let request = CreateMonitorRequest {
        user_id: UserId(Uuid::new_v4()),
        name: "Google".into(),
        url: "https://google.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 60,
        timeout_seconds: 10,
    };

    let monitor = use_case.execute(request).await.unwrap();

    assert_eq!(monitor.name.as_str(), "Google");
}
