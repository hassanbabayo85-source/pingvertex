use application::{
    commands::UploadStatusPageLogo, ports::StatusPageRepository,
    use_cases::UploadStatusPageLogoUseCase,
};
use domain::{
    entities::{CreateStatusPage, StatusPage},
    shared::ids::{StatusPageId, UserId},
    value_objects::Subdomain,
};
use infra::{InMemoryLogoStorage, InMemoryStatusPageRepository};

fn sample_page(user_id: UserId) -> StatusPage {
    StatusPage::create(CreateStatusPage {
        id: StatusPageId::new(),
        user_id,
        name: "Acme Status".to_string(),
        subdomain: Subdomain::new("acme").unwrap(),
        monitor_ids: vec![],
    })
    .unwrap()
}

#[tokio::test]
async fn uploads_valid_png_and_updates_status_page() {
    let repo = InMemoryStatusPageRepository::new();
    let storage = InMemoryLogoStorage::new();
    let user_id = UserId::new();
    let page = sample_page(user_id);
    repo.save(&page).await.unwrap();

    let use_case = UploadStatusPageLogoUseCase::new(repo.clone(), storage.clone());
    let png = b"\x89PNG\r\n\x1a\n".to_vec();

    let updated = use_case
        .execute(UploadStatusPageLogo {
            id: page.id,
            user_id,
            data: png,
        })
        .await
        .unwrap()
        .expect("status page should exist and logo should upload");

    let logo_url = updated.logo_url.as_deref().expect("logo_url should be set");
    assert!(logo_url.starts_with("https://cdn.test/logos/inmemory/"));
    assert!(logo_url.ends_with(".png"));

    let persisted = repo
        .find_by_id_for_user(page.id, user_id)
        .await
        .unwrap()
        .expect("status page should still exist");
    assert_eq!(persisted.logo_url.as_deref(), Some(logo_url));
}

#[tokio::test]
async fn returns_none_when_caller_does_not_own_status_page() {
    let repo = InMemoryStatusPageRepository::new();
    let storage = InMemoryLogoStorage::new();
    let owner = UserId::new();
    let attacker = UserId::new();
    let page = sample_page(owner);
    repo.save(&page).await.unwrap();

    let use_case = UploadStatusPageLogoUseCase::new(repo.clone(), storage.clone());

    let result = use_case
        .execute(UploadStatusPageLogo {
            id: page.id,
            user_id: attacker,
            data: b"\x89PNG\r\n\x1a\n".to_vec(),
        })
        .await
        .unwrap();

    assert!(result.is_none());
}
