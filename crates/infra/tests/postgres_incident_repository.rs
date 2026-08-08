mod common;

use application::ports::incident_repository::IncidentRepository;
use application::ports::monitor_repository::MonitorRepository;
use common::{sample_incident, sample_monitor, setup_pool};
use infra::{PostgresIncidentRepository, PostgresMonitorRepository};
use serial_test::serial;

#[serial]
#[tokio::test]
async fn save_and_find_by_id_round_trips_incident() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool);

    let monitor = sample_monitor("Google", "https://google.com");
    monitor_repository.save(&monitor).await.unwrap();

    let incident = sample_incident(monitor.id);
    incident_repository.save(&incident).await.unwrap();

    let found = incident_repository
        .find_by_id(incident.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, incident.id);
    assert_eq!(found.monitor_id, monitor.id);
    assert_eq!(found.resolved_at, None);
}

#[serial]
#[tokio::test]
async fn find_open_by_monitor_returns_only_unresolved_incident() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool);

    let monitor = sample_monitor("Google", "https://google.com");
    monitor_repository.save(&monitor).await.unwrap();

    let open = sample_incident(monitor.id);
    let mut resolved = sample_incident(monitor.id);
    resolved.resolve();

    incident_repository.save(&open).await.unwrap();
    incident_repository.save(&resolved).await.unwrap();

    let found = incident_repository
        .find_open_by_monitor(monitor.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, open.id);
    assert_eq!(found.resolved_at, None);
}

#[serial]
#[tokio::test]
async fn update_persists_resolution() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool);

    let monitor = sample_monitor("Google", "https://google.com");
    monitor_repository.save(&monitor).await.unwrap();

    let mut incident = sample_incident(monitor.id);
    incident_repository.save(&incident).await.unwrap();

    incident.resolve();
    incident_repository.update(&incident).await.unwrap();

    let found = incident_repository
        .find_by_id(incident.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, incident.id);
    assert!(found.resolved_at.is_some());
}

#[serial]
#[tokio::test]
async fn list_by_monitor_returns_incidents_for_monitor() {
    let pool = setup_pool().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool);

    let monitor_a = sample_monitor("Monitor A", "https://a.example.com");
    let monitor_b = sample_monitor("Monitor B", "https://b.example.com");
    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();

    let incident_a1 = sample_incident(monitor_a.id);
    let incident_a2 = sample_incident(monitor_a.id);
    let incident_b = sample_incident(monitor_b.id);

    incident_repository.save(&incident_a1).await.unwrap();
    incident_repository.save(&incident_a2).await.unwrap();
    incident_repository.save(&incident_b).await.unwrap();

    let incidents = incident_repository
        .list_by_monitor(monitor_a.id)
        .await
        .unwrap();

    assert_eq!(incidents.len(), 2);
    assert!(
        incidents
            .iter()
            .all(|incident| incident.monitor_id == monitor_a.id)
    );
}
