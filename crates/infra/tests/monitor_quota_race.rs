mod common;

use application::ports::monitor_repository::{MonitorRepository, QuotaCheckOutcome};
use common::{insert_user, setup_pool_exclusive};
use domain::shared::ids::{MonitorId, UserId};
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::PostgresMonitorRepository;
use serial_test::serial;
use uuid::Uuid;

/// Atomic Quota Enforcement regression test.
///
/// Spawns far more concurrent `save_if_within_quota` calls for the SAME
/// tenant than the quota allows, and asserts that exactly `LIMIT` of them
/// succeed and the rest report `QuotaExceeded`.
///
/// This is the test the atomic-quota mechanism exists to satisfy: a naive
/// `find_all_for_tenant().len() >= limit` check followed by a separate
/// `save()` call has a classic COUNT -> COMPARE -> INSERT race — under
/// concurrency, many callers can all observe a count just under the
/// limit and all proceed to insert, ending up well over it. Because
/// `save_if_within_quota` takes a per-tenant Postgres advisory lock
/// (`pg_advisory_xact_lock`) before counting, every concurrent call for
/// this tenant is serialized, so the total saved can never exceed
/// `LIMIT` no matter how many callers race it at once.
#[serial]
#[tokio::test]
async fn concurrent_creates_never_exceed_quota() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let repository = PostgresMonitorRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user(&pool, user_id).await;

    const LIMIT: u32 = 5;
    const ATTEMPTS: u32 = 20;

    let mut tasks = Vec::with_capacity(ATTEMPTS as usize);

    for i in 0..ATTEMPTS {
        let repository = repository.clone();
        tasks.push(tokio::spawn(async move {
            let monitor = MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
                .name(MonitorName::new(format!("Monitor {i}")).unwrap())
                .url(Url::new(format!("https://example{i}.com")).unwrap())
                .method(HttpMethod::Get)
                .expected_status(ExpectedStatus::new(200).unwrap())
                .interval(CheckInterval::new(60).unwrap())
                .timeout(Timeout::new(10).unwrap())
                .build();

            repository
                .save_if_within_quota(&monitor, tenant_id, LIMIT)
                .await
                .unwrap()
        }));
    }

    let mut saved = 0u32;
    let mut exceeded = 0u32;
    for task in tasks {
        match task.await.unwrap() {
            QuotaCheckOutcome::Saved => saved += 1,
            QuotaCheckOutcome::QuotaExceeded => exceeded += 1,
        }
    }

    assert_eq!(
        saved, LIMIT,
        "expected exactly {LIMIT} concurrent saves to succeed, got {saved}"
    );
    assert_eq!(exceeded, ATTEMPTS - LIMIT);

    let monitors = repository.find_all_for_tenant(tenant_id).await.unwrap();
    assert_eq!(
        monitors.len() as u32,
        LIMIT,
        "final row count in the database must match the quota exactly, not exceed it"
    );
}

/// Confirms a rejected `save_if_within_quota` call is a true no-op: no
/// row is written for the rejected monitor, and an existing tenant's
/// saved-row count is left untouched. Guards against a subtly wrong
/// implementation that inserts first and then "undoes" the insert on
/// paths that don't actually roll back cleanly.
#[serial]
#[tokio::test]
async fn quota_exceeded_does_not_persist_a_partial_row() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let repository = PostgresMonitorRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = insert_user(&pool, user_id).await;

    let first = MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("First").unwrap())
        .url(Url::new("https://first.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build();

    let outcome = repository
        .save_if_within_quota(&first, tenant_id, 1)
        .await
        .unwrap();
    assert_eq!(outcome, QuotaCheckOutcome::Saved);

    let second = MonitorBuilder::new(MonitorId(Uuid::new_v4()), user_id, tenant_id)
        .name(MonitorName::new("Second").unwrap())
        .url(Url::new("https://second.example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .build();

    let outcome = repository
        .save_if_within_quota(&second, tenant_id, 1)
        .await
        .unwrap();
    assert_eq!(outcome, QuotaCheckOutcome::QuotaExceeded);

    let found = repository.find_by_id(second.id).await.unwrap();
    assert!(
        found.is_none(),
        "a rejected save must not leave a row behind for the rejected monitor"
    );

    let monitors = repository.find_all_for_tenant(tenant_id).await.unwrap();
    assert_eq!(
        monitors.len(),
        1,
        "the existing saved monitor must be the only row for this tenant"
    );
}
