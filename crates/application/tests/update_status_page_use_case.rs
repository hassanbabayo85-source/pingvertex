use application::commands::UpdateStatusPage;
use application::dto::CreateStatusPageRequest;
use application::use_cases::{CreateStatusPageUseCase, UpdateStatusPageUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn updates_status_page_successfully() {
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

    let update_use_case = UpdateStatusPageUseCase::new(repository.clone());

    let updated = update_use_case
        .execute(UpdateStatusPage {
            id: status_page.id,
            user_id,
            name: "Acme Status Page".into(),
            monitor_ids: vec![],
            logo_url: Some("https://cdn.example.com/logo.png".into()),
            primary_color: Some("#336699".into()),
        })
        .await
        .unwrap()
        .expect("status page should exist");

    assert_eq!(updated.name, "Acme Status Page");
    assert_eq!(
        updated.logo_url.as_deref(),
        Some("https://cdn.example.com/logo.png")
    );
    assert!(updated.primary_color.is_some());
}

#[tokio::test]
async fn cannot_update_status_page_owned_by_another_user() {
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

    let update_use_case = UpdateStatusPageUseCase::new(repository.clone());

    let result = update_use_case
        .execute(UpdateStatusPage {
            id: status_page.id,
            user_id: attacker_id,
            name: "Hijacked".into(),
            monitor_ids: vec![],
            logo_url: None,
            primary_color: None,
        })
        .await
        .unwrap();

    assert!(result.is_none());
}
