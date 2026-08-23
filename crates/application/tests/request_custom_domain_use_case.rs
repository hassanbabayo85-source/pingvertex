mod common;
use common::*;

use application::commands::RequestCustomDomain;
use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{
    CreateStatusPageUseCase, RequestCustomDomainError, RequestCustomDomainUseCase,
};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn request_custom_domain_starts_in_pending_state() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );
    let user_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let status_page = create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Status".into(),
                subdomain: "acme".into(),
                monitor_ids: vec![],
            },
            auth_user(user_id, tid),
        )
        .await
        .unwrap();

    let request_domain_use_case =
        RequestCustomDomainUseCase::new(repository.clone(), new_audit_repo());

    let updated = request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: status_page.id,
                domain: "status.acme.com".into(),
            },
            auth_user(user_id, tid),
        )
        .await
        .unwrap()
        .expect("status page should exist");

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
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );
    let owner_id = UserId(Uuid::new_v4());
    let owner_tenant_id = tenant_id();
    let attacker_id = UserId(Uuid::new_v4());
    let attacker_tenant_id = tenant_id();

    let status_page = create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Status".into(),
                subdomain: "acme".into(),
                monitor_ids: vec![],
            },
            auth_user(owner_id, owner_tenant_id),
        )
        .await
        .unwrap();

    let request_domain_use_case =
        RequestCustomDomainUseCase::new(repository.clone(), new_audit_repo());

    let result = request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: status_page.id,
                domain: "status.evil.com".into(),
            },
            auth_user(attacker_id, attacker_tenant_id),
        )
        .await
        .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn cannot_claim_a_domain_already_verified_by_another_tenant() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );

    let owner_id = UserId(Uuid::new_v4());
    let owner_tenant_id = tenant_id();
    let other_id = UserId(Uuid::new_v4());
    let other_tenant_id = tenant_id();

    let owner_page = create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Status".into(),
                subdomain: "acme".into(),
                monitor_ids: vec![],
            },
            auth_user(owner_id, owner_tenant_id),
        )
        .await
        .unwrap();

    let other_page = create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Other Status".into(),
                subdomain: "other".into(),
                monitor_ids: vec![],
            },
            auth_user(other_id, other_tenant_id),
        )
        .await
        .unwrap();

    let request_domain_use_case =
        RequestCustomDomainUseCase::new(repository.clone(), new_audit_repo());

    // Owner claims and verifies "status.shared.com".
    let mut owned = request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: owner_page.id,
                domain: "status.shared.com".into(),
            },
            auth_user(owner_id, owner_tenant_id),
        )
        .await
        .unwrap()
        .expect("status page should exist");

    owned.mark_custom_domain_verified().unwrap();
    repository.update(&owned).await.unwrap();

    // A different tenant tries to claim the same, now-verified domain.
    let result = request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: other_page.id,
                domain: "status.shared.com".into(),
            },
            auth_user(other_id, other_tenant_id),
        )
        .await;

    assert!(matches!(
        result,
        Err(RequestCustomDomainError::AlreadyClaimed { .. })
    ));

    // The other tenant's status page must be left untouched.
    let persisted = repository
        .find_by_id_for_user(other_page.id, other_id)
        .await
        .unwrap()
        .expect("status page should exist in repository");
    assert!(persisted.custom_domain.is_none());
}
