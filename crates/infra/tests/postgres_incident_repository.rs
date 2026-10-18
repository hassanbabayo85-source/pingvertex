mod common;

use application::ports::incident_repository::IncidentRepository;
use application::ports::monitor_repository::MonitorRepository;
use common::{sample_incident, sample_monitor, setup_pool_exclusive};
use infra::{PostgresIncidentRepository, PostgresMonitorRepository};
use serial_test::serial;

#[serial]
#[tokio::test]
async fn save_and_find_by_id_round_trips_incident() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
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
async fn save_does_not_create_duplicate_open_incidents_for_same_monitor() {
    // SEC-010 regression test: if more than one worker replica tries to open
    // an incident for the same monitor at the same time (a race), both call
    // `save` with a *different* incident (different id) that is nonetheless
    // open (resolved_at = None) for that one monitor. The partial unique index
    // should block the second one (via ON CONFLICT DO NOTHING) rather than
    // raising an error or creating a duplicate.
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());

    let monitor = sample_monitor(
        &pool,
        "Duplicate Guard",
        "https://duplicate-guard.example.com",
    )
    .await;
    monitor_repository.save(&monitor).await.unwrap();

    let first_open = sample_incident(monitor.id);
    let second_open = sample_incident(monitor.id);

    // Neither of these has resolved_at set — both are open.
    assert!(first_open.resolved_at.is_none());
    assert!(second_open.resolved_at.is_none());

    incident_repository.save(&first_open).await.unwrap();

    // This will NOT raise an error even though it violates the constraint —
    // ON CONFLICT DO NOTHING makes it a silent no-op.
    incident_repository.save(&second_open).await.unwrap();

    let open_incidents = incident_repository
        .list_by_monitor(monitor.id)
        .await
        .unwrap();

    // Exactly one incident should exist — the second was silently discarded
    // by ON CONFLICT DO NOTHING.
    assert_eq!(open_incidents.len(), 1);
    assert_eq!(open_incidents[0].id, first_open.id);

    let still_open = incident_repository
        .find_open_by_monitor(monitor.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still_open.id, first_open.id);
}

#[serial]
#[tokio::test]
async fn find_open_by_monitor_returns_only_unresolved_incident() {
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
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
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());

    let monitor = sample_monitor(&pool, "Google", "https://google.com").await;
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
    let (pool, _guard) = setup_pool_exclusive().await;
    let monitor_repository = PostgresMonitorRepository::new(pool.clone());
    let incident_repository = PostgresIncidentRepository::new(pool.clone());

    let monitor_a = sample_monitor(&pool, "Monitor A", "https://a.example.com").await;
    let monitor_b = sample_monitor(&pool, "Monitor B", "https://b.example.com").await;
    monitor_repository.save(&monitor_a).await.unwrap();
    monitor_repository.save(&monitor_b).await.unwrap();

    let mut incident_a1 = sample_incident(monitor_a.id);
    let incident_a2 = sample_incident(monitor_a.id);
    let incident_b = sample_incident(monitor_b.id);

    // SEC-010: the database no longer allows more than one open incident
    // (resolved_at IS NULL) per monitor at a time. So that this test can still
    // verify `list_by_monitor` (which returns ALL incidents, not only open
    // ones), we resolve incident_a1 before opening incident_a2 — which matches
    // real usage anyway (there are never two simultaneously open incidents for
    // one monitor).
    incident_a1.resolve();

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
