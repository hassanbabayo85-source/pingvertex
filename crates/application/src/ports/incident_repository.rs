use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Abstraction for incident persistence.
#[async_trait]
pub trait IncidentRepository: Send + Sync {
    async fn save(&self, incident: &Incident) -> RepoResult<()>;

    async fn update(&self, incident: &Incident) -> RepoResult<()>;

    async fn find_by_id(&self, id: IncidentId) -> RepoResult<Option<Incident>>;

    async fn find_open_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Option<Incident>>;

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Incident>>;
}

#[async_trait]
impl<T> IncidentRepository for Arc<T>
where
    T: IncidentRepository + ?Sized + Send + Sync,
{
    async fn save(&self, incident: &Incident) -> RepoResult<()> {
        (**self).save(incident).await
    }

    async fn update(&self, incident: &Incident) -> RepoResult<()> {
        (**self).update(incident).await
    }

    async fn find_by_id(&self, id: IncidentId) -> RepoResult<Option<Incident>> {
        (**self).find_by_id(id).await
    }

    async fn find_open_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Option<Incident>> {
        (**self).find_open_by_monitor(monitor_id).await
    }

    async fn list_by_monitor(&self, monitor_id: MonitorId) -> RepoResult<Vec<Incident>> {
        (**self).list_by_monitor(monitor_id).await
    }
}
