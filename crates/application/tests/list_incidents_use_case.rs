use std::sync::Arc;

use application::{
    ports::{IncidentRepository, incident_repository::RepoResult as IncidentRepoResult},
    use_cases::ListIncidentsUseCase,
};
use async_trait::async_trait;
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId};

#[derive(Clone, Default)]
struct NoopIncidentRepository;

#[async_trait]
impl IncidentRepository for NoopIncidentRepository {
    async fn save(&self, _: &Incident) -> IncidentRepoResult<()> {
        Ok(())
    }

    async fn update(&self, _: &Incident) -> IncidentRepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _: IncidentId) -> IncidentRepoResult<Option<Incident>> {
        Ok(None)
    }

    async fn find_open_by_monitor(&self, _: MonitorId) -> IncidentRepoResult<Option<Incident>> {
        Ok(None)
    }

    async fn list_by_monitor(&self, _: MonitorId) -> IncidentRepoResult<Vec<Incident>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn lists_incidents_for_monitor() {
    let repository = Arc::new(NoopIncidentRepository);
    let use_case = ListIncidentsUseCase::new(repository);

    let result = use_case
        .execute(MonitorId(uuid::Uuid::new_v4()))
        .await
        .unwrap();

    assert!(result.is_empty());
}
