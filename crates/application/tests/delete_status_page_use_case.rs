use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{CreateStatusPageUseCase, DeleteStatusPageUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn deletes_status_page_successfully() {
    let repository = InMemoryStatusPageRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new());
    let user_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone());

    let deleted = delete_use_case
        .execute(status_page.id, user_id)
        .await
        .unwrap();

    assert!(deleted);

    let remaining = repository
        .find_by_id_for_user(status_page.id, user_id)
        .await
        .unwrap();

    assert!(remaining.is_none());
}

#[tokio::test]
async fn deleting_someone_elses_status_page_returns_false() {
    let repository = InMemoryStatusPageRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new());
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());

    let status_page = create_use_case
        .execute(CreateStatusPageRequest {
            user_id: owner_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone());

    let deleted = delete_use_case
        .execute(status_page.id, attacker_id)
        .await
        .unwrap();

    assert!(!deleted);
}
