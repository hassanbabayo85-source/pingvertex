mod common;
use common::*;

use std::sync::Arc;

use application::{
    dto::CreateMonitorRequest,
    ports::{IncidentRepository, incident_repository::RepoResult as IncidentRepoResult},
    use_cases::{CreateMonitorUseCase, ListIncidentsUseCase},
};
use async_trait::async_trait;
use domain::HttpMethod;
use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId, UserId};
use infra::{InMemoryMonitorRepository, UuidIdGenerator};
use uuid::Uuid;

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
    let monitor_repository = Arc::new(InMemoryMonitorRepository::new());
    let incident_repository = Arc::new(NoopIncidentRepository);

    let create_use_case = CreateMonitorUseCase::new(
        monitor_repository.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    );

    let user_id = UserId(Uuid::new_v4());
    let tenant_id = tenant_id();

    let request = CreateMonitorRequest {
        regions: Vec::new(),
        name: "Test Monitor".into(),
        url: "https://example.com".into(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 300,
        timeout_seconds: 10,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: true,
        ssl_expiry_warning_days: 14,
        kind: domain::MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    };

    let monitor = create_use_case
        .execute(request, auth_user(user_id, tenant_id))
        .await
        .unwrap();

    let use_case = ListIncidentsUseCase::new(monitor_repository, incident_repository);

    let result = use_case
        .execute(monitor.id, auth_user(user_id, tenant_id))
        .await
        .unwrap();
    assert!(result.is_empty());
}
