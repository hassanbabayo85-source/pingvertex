mod common;
use common::*;

use application::dto::CreateStatusPageRequest;
use application::ports::MonitorRepository;
use application::use_cases::{CreateStatusPageError, CreateStatusPageUseCase};
use domain::shared::ids::UserId;
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, Monitor, MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::{InMemoryMonitorRepository, InMemoryStatusPageRepository, UuidIdGenerator};
use uuid::Uuid;

fn request(_user_id: UserId, subdomain: &str) -> CreateStatusPageRequest {
    CreateStatusPageRequest {
        name: "Acme Status".into(),
        subdomain: subdomain.into(),
        monitor_ids: vec![],
    }
}

fn use_case(
    status_pages: InMemoryStatusPageRepository,
    monitors: InMemoryMonitorRepository,
) -> CreateStatusPageUseCase<InMemoryStatusPageRepository, UuidIdGenerator, InMemoryMonitorRepository>
{
    CreateStatusPageUseCase::new(
        status_pages,
        UuidIdGenerator::new(),
        monitors,
        new_audit_repo(),
    )
}

fn sample_monitor(user_id: UserId, tenant_id: domain::TenantId) -> Monitor {
    MonitorBuilder::new(domain::MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("Acme API").unwrap())
        .url(Url::new("https://api.acme.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build()
}

#[tokio::test]
async fn create_status_page_persists_status_page() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());

    let status_page = use_case
        .execute(request(user_id, "acme"), auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    assert_eq!(status_page.name, "Acme Status");
    assert_eq!(status_page.subdomain.as_str(), "acme");
}

#[tokio::test]
async fn rejects_duplicate_subdomain() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());
    let other_user_id = UserId(Uuid::new_v4());

    use_case
        .execute(request(user_id, "acme"), auth_user(user_id, tenant_id()))
        .await
        .unwrap();
    let result = use_case
        .execute(
            request(other_user_id, "acme"),
            auth_user(other_user_id, tenant_id()),
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rejects_creation_past_quota() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let use_case = use_case(repository.clone(), monitors);
    let user_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    for i in 0..application::use_cases::create_status_page::MAX_STATUS_PAGES_PER_USER {
        use_case
            .execute(
                request(user_id, &format!("acme-{i}")),
                auth_user(user_id, tid),
            )
            .await
            .unwrap();
    }
    let result = use_case
        .execute(request(user_id, "one-too-many"), auth_user(user_id, tid))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rejects_monitor_not_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let owner_id = UserId(Uuid::new_v4());
    let attacker_id = UserId(Uuid::new_v4());
    let tid = tenant_id();

    let victim_monitor = sample_monitor(owner_id, tid);
    monitors.save(&victim_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);
    let result = use_case
        .execute(
            CreateStatusPageRequest {
                name: "Attacker Status".into(),
                subdomain: "attacker".into(),
                monitor_ids: vec![victim_monitor.id],
            },
            auth_user(attacker_id, tid),
        )
        .await;

    assert!(matches!(
        result,
        Err(CreateStatusPageError::MonitorNotFound { .. })
    ));
}

#[tokio::test]
async fn allows_monitor_owned_by_the_requesting_user() {
    let repository = InMemoryStatusPageRepository::new();
    let monitors = InMemoryMonitorRepository::new();
    let owner_id = UserId(Uuid::new_v4());
    let tid = tenant_id();
    let own_monitor = sample_monitor(owner_id, tid);
    monitors.save(&own_monitor).await.unwrap();

    let use_case = use_case(repository, monitors);
    let status_page = use_case
        .execute(
            CreateStatusPageRequest {
                name: "Acme Status".into(),
                subdomain: "acme".into(),
                monitor_ids: vec![own_monitor.id],
            },
            auth_user(owner_id, tid),
        )
        .await
        .unwrap();

    assert_eq!(status_page.monitor_ids, vec![own_monitor.id]);
}
