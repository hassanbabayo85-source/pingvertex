mod common;

use application::ports::check_repository::CheckRepository;
use application::ports::monitor_repository::MonitorRepository;
use common::{sample_check, sample_monitor, setup_pool};
use domain::{CheckStatus, ErrorType};
use infra::{PostgresCheckRepository, PostgresMonitorRepository};
use serial_test::serial;

#[serial]
#[tokio::test]
async fn save_and_find_by_id_round_trips_check() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
    monitor_repository.save(&monitor).await.unwrap();

    let check = sample_check(monitor.id);
    check_repository.save(&check).await.unwrap();

    let found = check_repository
        .find_by_id(check.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, check.id);
    assert_eq!(found.monitor_id, monitor.id);
    assert_eq!(found.status, CheckStatus::Up);
    assert_eq!(found.status_code, Some(200));
    assert_eq!(found.latency_ms, 123);
    assert_eq!(found.error_type, ErrorType::None);
    assert_eq!(found.error_message, None);
}

#[serial]
#[tokio::test]
async fn list_by_monitor_returns_only_matching_checks() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor_a = sample_monitor(&pool, "Monitor A", "https://a.example.com").await;
    let monitor_b = sample_monitor(&pool, "Monitor B", "https://b.example.com").await;
    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();

    let check_a = sample_check(monitor_a.id);
    let check_b = sample_check(monitor_b.id);

    check_repository.save(&check_a).await.unwrap();
    check_repository.save(&check_b).await.unwrap();

    let checks = check_repository
        .list_by_monitor(monitor_a.id)
        .await
        .unwrap();

    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].id, check_a.id);
}

#[serial]
#[tokio::test]
async fn delete_by_monitor_removes_all_checks_for_monitor() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let check_repository = PostgresCheckRepository::new(pool.clone());

    let monitor_a = sample_monitor(&pool, "Monitor A", "https://a.example.com").await;
    let monitor_b = sample_monitor(&pool, "Monitor B", "https://b.example.com").await;
    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();

    let check_a1 = sample_check(monitor_a.id);
    let check_a2 = sample_check(monitor_a.id);
    let check_b = sample_check(monitor_b.id);

    check_repository.save(&check_a1).await.unwrap();
    check_repository.save(&check_a2).await.unwrap();
    check_repository.save(&check_b).await.unwrap();

    let deleted = check_repository
        .delete_by_monitor(monitor_a.id)
        .await
        .unwrap();

    assert_eq!(deleted, 2);
    assert!(
        check_repository
            .list_by_monitor(monitor_a.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        check_repository
            .list_by_monitor(monitor_b.id)
            .await
            .unwrap()
            .len(),
        1
    );
}
