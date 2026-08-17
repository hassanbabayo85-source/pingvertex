use application::commands::RequestCustomDomain;
use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{CreateStatusPageUseCase, RequestCustomDomainUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn request_custom_domain_starts_in_pending_state() {
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

    let request_domain_use_case = RequestCustomDomainUseCase::new(repository.clone());

    let updated = request_domain_use_case
        .execute(RequestCustomDomain {
            id: status_page.id,
            user_id,
            domain: "status.acme.com".into(),
        })
        .await
        .unwrap()
        .expect("status page should exist");

    // Freshly requested — NOT verified yet, and therefore must not be
    // usable for host-header routing. This is the security invariant the
    // whole custom-domain feature depends on.
    assert!(updated.verified_custom_domain().is_none());

    let persisted = repository
        .find_by_id_for_user(status_page.id, user_id)
        .await
        .unwrap()
        .expect("status page should exist in repository");

    assert!(persisted.verified_custom_domain().is_none());
    assert!(persisted.custom_domain.is_some());
}

#[tokio::test]
async fn cannot_request_custom_domain_for_someone_elses_status_page() {
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

    let request_domain_use_case = RequestCustomDomainUseCase::new(repository.clone());

    let result = request_domain_use_case
        .execute(RequestCustomDomain {
            id: status_page.id,
            user_id: attacker_id,
            domain: "status.evil.com".into(),
        })
        .await
        .unwrap();

    assert!(result.is_none());
}
