mod common;

use application::ports::monitor_repository::MonitorRepository;
use common::{sample_monitor, setup_pool};
use infra::PostgresMonitorRepository;
use serial_test::serial;

#[serial]
#[tokio::test]
async fn save_and_find_by_id_round_trips_monitor() {
    let pool = setup_pool().await;
    let repository = PostgresMonitorRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
    repository.save(&monitor).await.unwrap();

    let found = repository.find_by_id(monitor.id).await.unwrap().unwrap();

    assert_eq!(found.id, monitor.id);
    assert_eq!(found.user_id, monitor.user_id);
    assert_eq!(found.name.as_str(), monitor.name.as_str());
    assert_eq!(found.url, monitor.url);
    assert_eq!(found.method, monitor.method);
    assert_eq!(found.expected_status, monitor.expected_status);
    assert_eq!(found.interval_seconds, monitor.interval_seconds);
    assert_eq!(found.timeout_seconds, monitor.timeout_seconds);
    assert_eq!(found.enabled, monitor.enabled);
}

#[serial]
#[tokio::test]
async fn update_persists_changes() {
    let pool = setup_pool().await;
    let repository = PostgresMonitorRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
    repository.save(&monitor).await.unwrap();

    let mut updated = monitor.clone();
    updated.name = domain::MonitorName::new("Google Search").unwrap();
    updated.url = domain::Url::new("https://www.google.com")
        .unwrap()
        .as_str()
        .to_owned();
    updated.method = domain::HttpMethod::Head;
    updated.enabled = false;

    repository.update(&updated).await.unwrap();

    let found = repository.find_by_id(updated.id).await.unwrap().unwrap();

    assert_eq!(found.name.as_str(), "Google Search");
    assert_eq!(found.url, "https://www.google.com/");
    assert_eq!(found.method, domain::HttpMethod::Head);
    assert!(!found.enabled);
}

#[serial]
#[tokio::test]
async fn find_all_returns_saved_monitors() {
    let pool = setup_pool().await;
    let repository = PostgresMonitorRepository::new(pool.clone());

    let first = sample_monitor(&pool, "Monitor 1", "https://example1.com").await;
    let second = sample_monitor(&pool, "Monitor 2", "https://example2.com").await;

    repository.save(&first).await.unwrap();
    repository.save(&second).await.unwrap();

    let monitors = repository.find_all().await.unwrap();

    assert_eq!(monitors.len(), 2);
    let names: Vec<_> = monitors
        .iter()
        .map(|m| m.name.as_str().to_string())
        .collect();
    assert!(names.contains(&"Monitor 1".to_string()));
    assert!(names.contains(&"Monitor 2".to_string()));
}

#[serial]
#[tokio::test]
async fn delete_removes_monitor() {
    let pool = setup_pool().await;
    let repository = PostgresMonitorRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Delete Me", "https://deleteme.com").await;
    repository.save(&monitor).await.unwrap();

    let deleted = repository.delete(monitor.id).await.unwrap();
    let found = repository.find_by_id(monitor.id).await.unwrap();

    assert!(deleted);
    assert!(found.is_none());
}
