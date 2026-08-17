use application::dto::CreateStatusPageRequest;
use application::use_cases::CreateStatusPageUseCase;
use application::use_cases::create_status_page::MAX_STATUS_PAGES_PER_USER;
use domain::shared::ids::UserId;
use infra::{InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

fn request(user_id: UserId, subdomain: &str) -> CreateStatusPageRequest {
    CreateStatusPageRequest {
        user_id,
        name: "Acme Status".into(),
        subdomain: subdomain.into(),
        monitor_ids: vec![],
    }
}

#[tokio::test]
async fn create_status_page_persists_status_page() {
    let repository = InMemoryStatusPageRepository::new();
    let use_case = CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new());
    let user_id = UserId(Uuid::new_v4());

    let status_page = use_case.execute(request(user_id, "acme")).await.unwrap();

    assert_eq!(status_page.name, "Acme Status");
    assert_eq!(status_page.subdomain.as_str(), "acme");
}

#[tokio::test]
async fn rejects_duplicate_subdomain() {
    let repository = InMemoryStatusPageRepository::new();
    let use_case = CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new());
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());

    use_case.execute(request(user_id, "acme")).await.unwrap();

    // Even a different user cannot take an already-used subdomain —
    // subdomains are globally unique, not per-user.
    let result = use_case.execute(request(other_user_id, "acme")).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn rejects_creation_past_quota() {
    let repository = InMemoryStatusPageRepository::new();
    let use_case = CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new());
    let user_id = UserId(Uuid::new_v4());

    for i in 0..MAX_STATUS_PAGES_PER_USER {
        use_case
            .execute(request(user_id, &format!("acme-{i}")))
            .await
            .unwrap();
    }

    let result = use_case.execute(request(user_id, "one-too-many")).await;

    assert!(result.is_err());
}
