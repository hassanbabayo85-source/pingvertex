mod common;
use common::*;

use application::dto::CreateStatusPageRequest;
use application::ports::StatusPageRepository;
use application::use_cases::{CreateStatusPageUseCase, DeleteStatusPageUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn deletes_status_page_successfully() {
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

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(status_page.id, auth_user(user_id, tid))
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
async fn deleting_status_page_as_another_tenant_member_succeeds() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );
    let owner_id = UserId(Uuid::new_v4());
    let tenant_mate_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let status_page = create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Status".into(),
                subdomain: "acme".into(),
                monitor_ids: vec![],
            },
            auth_user(owner_id, tid),
        )
        .await
        .unwrap();

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(status_page.id, auth_user(tenant_mate_id, tid))
        .await
        .unwrap();

    assert!(deleted);

    let remaining = repository.find_by_id(status_page.id).await.unwrap();
    assert!(remaining.is_none());
}

#[tokio::test]
async fn deleting_status_page_from_another_tenant_returns_false() {
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

    let delete_use_case = DeleteStatusPageUseCase::new(repository.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(status_page.id, auth_user(attacker_id, attacker_tenant_id))
        .await
        .unwrap();

    assert!(!deleted);

    let remaining = repository.find_by_id(status_page.id).await.unwrap();
    assert!(remaining.is_some());
}
