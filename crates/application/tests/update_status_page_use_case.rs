mod common;
use common::*;

use application::commands::UpdateStatusPage;
use application::dto::CreateStatusPageRequest;
use application::ports::MonitorRepository;
use application::use_cases::{
    CreateStatusPageUseCase, UpdateStatusPageError, UpdateStatusPageUseCase,
};
use domain::shared::ids::{MonitorId, UserId};
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, Monitor, MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

fn sample_monitor(user_id: UserId, tenant_id: domain::TenantId) -> Monitor {
    MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("Acme API").unwrap())
        .url(Url::new("https://api.acme.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build()
}

#[tokio::test]
async fn updates_status_page_successfully() {
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

    let update_use_case =
        UpdateStatusPageUseCase::new(repository.clone(), monitors, new_audit_repo());

    let updated = update_use_case
        .execute(
            UpdateStatusPage {
                id: status_page.id,
                name: "Acme Status Page".into(),
                monitor_ids: vec![],
                logo_url: Some("https://cdn.example.com/logo.png".into()),
                primary_color: Some("#336699".into()),
            },
            auth_user(user_id, tid),
        )
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
    let monitors = InMemoryMonitorRepository::new();
    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());
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

    let update_use_case =
        UpdateStatusPageUseCase::new(repository.clone(), monitors, new_audit_repo());

    let result = update_use_case
        .execute(
            UpdateStatusPage {
                id: status_page.id,
                name: "Hijacked".into(),
                monitor_ids: vec![],
                logo_url: None,
                primary_color: None,
            },
            auth_user(attacker_id, tid),
        )
        .await
        .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn rejects_adding_a_monitor_not_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();

    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let create_use_case = CreateStatusPageUseCase::new(
        repository.clone(),
        UuidIdGenerator::new(),
        monitors.clone(),
        new_audit_repo(),
    );

    let status_page = create_use_case
        .execute(
            CreateStatusPageRequest {
                name: "Attacker Status".into(),
                subdomain: "attacker".into(),
                monitor_ids: vec![],
            },
            auth_user(attacker_id, tid),
        )
        .await
        .unwrap();

    let victim_monitor = sample_monitor(owner_id, tid);
    monitors.save(&victim_monitor).await.unwrap();

    let update_use_case = UpdateStatusPageUseCase::new(repository, monitors, new_audit_repo());

    let result = update_use_case
        .execute(
            UpdateStatusPage {
                id: status_page.id,
                name: "Attacker Status".into(),
                monitor_ids: vec![victim_monitor.id],
                logo_url: None,
                primary_color: None,
            },
            auth_user(attacker_id, tid),
        )
        .await;

    assert!(matches!(
        result,
        Err(UpdateStatusPageError::MonitorNotFound { .. })
    ));
}
