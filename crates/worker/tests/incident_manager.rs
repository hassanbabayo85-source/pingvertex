use std::sync::{Arc, Mutex};

use application::ports::IncidentRepository;
use async_trait::async_trait;
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId};
use uuid::Uuid;

use worker::IncidentManager;

#[derive(Default)]
struct RepoState {
    open_incident_exists: bool,
    save_calls: usize,
    update_calls: usize,
    last_saved: Option<Incident>,
    last_updated: Option<Incident>,
}

#[derive(Clone)]
struct MockIncidentRepository {
    state: Arc<Mutex<RepoState>>,
}

impl MockIncidentRepository {
    fn new(open_incident_exists: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(RepoState {
                open_incident_exists,
                ..Default::default()
            })),
        }
    }

    fn save_calls(&self) -> usize {
        self.state.lock().unwrap().save_calls
    }

    fn update_calls(&self) -> usize {
        self.state.lock().unwrap().update_calls
    }
}

#[async_trait]
impl IncidentRepository for MockIncidentRepository {
    async fn save(
        &self,
        incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.lock().unwrap();
        state.save_calls += 1;
        state.last_saved = Some(incident.clone());
        Ok(())
    }

    async fn update(
        &self,
        incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.lock().unwrap();
        state.update_calls += 1;
        state.last_updated = Some(incident.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn find_open_by_monitor(
        &self,
        monitor_id: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        let state = self.state.lock().unwrap();

        if state.open_incident_exists {
            Ok(Some(Incident::new(IncidentId(Uuid::new_v4()), monitor_id)))
        } else {
            Ok(None)
        }
    }

    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn opens_incident_when_none_exists() {
    let repository = MockIncidentRepository::new(false);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());

    let manager = IncidentManager::new(Arc::new(repository.clone()), id_generator);

    manager
        .open_if_needed(MonitorId(Uuid::new_v4()))
        .await
        .unwrap();

    assert_eq!(repository.save_calls(), 1);
    assert_eq!(repository.update_calls(), 0);
}

#[tokio::test]
async fn does_not_open_duplicate_incident() {
    let repository = MockIncidentRepository::new(true);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());

    let manager = IncidentManager::new(Arc::new(repository.clone()), id_generator);

    manager
        .open_if_needed(MonitorId(Uuid::new_v4()))
        .await
        .unwrap();

    assert_eq!(repository.save_calls(), 0);
    assert_eq!(repository.update_calls(), 0);
}

#[tokio::test]
async fn resolves_open_incident() {
    let repository = MockIncidentRepository::new(true);
    let id_generator = Arc::new(infra::UuidIdGenerator::new());

    let manager = IncidentManager::new(Arc::new(repository.clone()), id_generator);

    manager
        .resolve_if_needed(MonitorId(Uuid::new_v4()))
        .await
        .unwrap();

    assert_eq!(repository.save_calls(), 0);
    assert_eq!(repository.update_calls(), 1);
}
