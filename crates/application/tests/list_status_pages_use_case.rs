use application::dto::CreateStatusPageRequest;
use application::use_cases::{CreateStatusPageUseCase, ListStatusPagesUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn lists_only_status_pages_owned_by_user() {
    let repository = InMemoryStatusPageRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(repository.clone(), UuidIdGenerator::new());
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());

    create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Status".into(),
            subdomain: "acme".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    create_use_case
        .execute(CreateStatusPageRequest {
            user_id,
            name: "Acme Internal Status".into(),
            subdomain: "acme-internal".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    create_use_case
        .execute(CreateStatusPageRequest {
            user_id: other_user_id,
            name: "Widgets Inc Status".into(),
            subdomain: "widgets".into(),
            monitor_ids: vec![],
        })
        .await
        .unwrap();

    let list_use_case = ListStatusPagesUseCase::new(repository.clone());

    let pages = list_use_case.execute(user_id).await.unwrap();

    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|page| page.user_id == user_id));
}
