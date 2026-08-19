mod common;
use common::*;

use application::commands::RequestCustomDomain;
use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{CreateStatusPageUseCase, RequestCustomDomainUseCase};
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
