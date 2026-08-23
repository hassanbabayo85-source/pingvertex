mod common;
use common::*;

use application::dto::CreateStatusPageRequest;
use application::use_cases::{CreateStatusPageUseCase, ListStatusPagesUseCase};
use domain::shared::ids::UserId;
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

#[tokio::test]
async fn lists_only_status_pages_in_tenant() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());
    let tid = tenant_id();
    let other_tid = tenant_id();

    create_use_case
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

    create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Internal Status".into(),
                subdomain: "acme-internal".into(),
                monitor_ids: vec![],
            },
            auth_user(user_id, tid),
        )
        .await
        .unwrap();

    create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Widgets Inc Status".into(),
                subdomain: "widgets".into(),
                monitor_ids: vec![],
            },
            auth_user(other_user_id, other_tid),
        )
        .await
        .unwrap();

    let list_use_case = ListStatusPagesUseCase::new(repository.clone());
    let pages = list_use_case
        .execute(auth_user(user_id, tid))
        .await
        .unwrap();

    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|page| page.user_id == user_id));
}
