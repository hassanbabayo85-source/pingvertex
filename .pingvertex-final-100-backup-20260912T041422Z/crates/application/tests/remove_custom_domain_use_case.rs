mod common;
use common::*;

use application::commands::{RemoveCustomDomain, RequestCustomDomain};
use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{
    CreateStatusPageUseCase, RemoveCustomDomainUseCase, RequestCustomDomainUseCase,
};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn remove_custom_domain_clears_pending_domain() {
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

    request_domain_use_case
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

    let remove_domain_use_case =
        RemoveCustomDomainUseCase::new(repository.clone(), new_audit_repo());

    let updated = remove_domain_use_case
        .execute(
            RemoveCustomDomain { id: status_page.id },
            auth_user(user_id, tid),
        )
        .await
        .unwrap()
        .expect("status page should exist");

    assert!(updated.custom_domain.is_none());

    let persisted = repository
        .find_by_id_for_user(status_page.id, user_id)
        .await
        .unwrap()
        .expect("status page should exist in repository");
    assert!(persisted.custom_domain.is_none());
}

#[tokio::test]
async fn removing_custom_domain_when_none_set_is_a_no_op() {
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

    let remove_domain_use_case =
        RemoveCustomDomainUseCase::new(repository.clone(), new_audit_repo());

    let updated = remove_domain_use_case
        .execute(
            RemoveCustomDomain { id: status_page.id },
            auth_user(user_id, tid),
        )
        .await
        .unwrap()
        .expect("status page should exist");

    assert!(updated.custom_domain.is_none());
}

#[tokio::test]
async fn cannot_remove_custom_domain_for_someone_elses_status_page() {
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

    request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: status_page.id,
                domain: "status.acme.com".into(),
            },
            auth_user(owner_id, owner_tenant_id),
        )
        .await
        .unwrap()
        .expect("status page should exist");

    let remove_domain_use_case =
        RemoveCustomDomainUseCase::new(repository.clone(), new_audit_repo());

    let result = remove_domain_use_case
        .execute(
            RemoveCustomDomain { id: status_page.id },
            auth_user(attacker_id, attacker_tenant_id),
        )
        .await
        .unwrap();

    assert!(result.is_none());

    // Domain must still be intact — the attacker's call must not have
    // mutated another tenant's status page.
    let persisted = repository
        .find_by_id_for_user(status_page.id, owner_id)
        .await
        .unwrap()
        .expect("status page should exist in repository");
    assert!(persisted.custom_domain.is_some());
}
