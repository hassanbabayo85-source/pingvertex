mod common;
use common::*;

use application::dto::CreateStatusPageRequest;
use application::use_cases::{CreateStatusPageUseCase, ListStatusPagesUseCase};
use domain::shared::ids::UserId;
use infra::{
    InMemoryEntitlementResolver, InMemoryMonitorRepository, InMemoryStatusPageRepository,
    UuidIdGenerator,
};
use std::sync::Arc;
use uuid::Uuid;

fn starter_resolver(tenant_id: domain::shared::ids::TenantId) -> Arc<InMemoryEntitlementResolver> {
    let starter = domain::Entitlements::starter_v1_fixture();
    Arc::new(InMemoryEntitlementResolver::default().with_override(tenant_id, starter))
}

#[tokio::test]
async fn lists_only_status_pages_in_tenant() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());
    let tid = tenant_id();
    let other_tid = tenant_id();

    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    )
    .with_entitlement_resolver(starter_resolver(tid));

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
