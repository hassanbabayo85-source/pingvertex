use api::{auth_state::AuthState, router::build_router, state::AppState};
use application::ports::{CheckRepository, IncidentRepository, MonitorRepository};
use application::ports::{PasswordHasher, TokenService, UserRepository};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::shared::ids::{CheckId, IncidentId, MonitorId};
use domain::User;
use domain::{Check, Incident, Monitor};
use infra::UuidIdGenerator;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct MockCheckRepository;

#[async_trait]
impl CheckRepository for MockCheckRepository {
    async fn save(&self, _: &Check) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _: CheckId,
    ) -> Result<Option<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn list_checks(
        &self,
        _: u32,
        _: u32,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn delete_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        Ok(0)
    }
}

#[derive(Clone, Default)]
struct NoopIncidentRepository;

#[async_trait]
impl IncidentRepository for NoopIncidentRepository {
    async fn save(&self, _: &Incident) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(&self, _: &Incident) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_open_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn list_by_monitor(
        &self,
        _: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopMonitorRepository;

#[async_trait]
impl MonitorRepository for NoopMonitorRepository {
    async fn save(&self, _: &Monitor) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(&self, _: &Monitor) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: MonitorId,
    ) -> Result<Option<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_all(&self) -> Result<Vec<Monitor>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    async fn delete(&self, _: MonitorId) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
}

#[derive(Clone, Default)]
struct NoopUserRepository;
#[async_trait]
impl UserRepository for NoopUserRepository {
    async fn create(&self, _: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(&self, _: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: &domain::UserId,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_email(
        &self,
        _: &domain::Email,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_username(
        &self,
        _: &domain::Username,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn delete(
        &self,
        _: &domain::UserId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn email_exists(
        &self,
        _: &domain::Email,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
    async fn username_exists(
        &self,
        _: &domain::Username,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
}

#[derive(Clone, Default)]
struct NoopPasswordHasher;
#[async_trait]
impl PasswordHasher for NoopPasswordHasher {
    async fn hash(&self, _: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(String::new())
    }
    async fn verify(
        &self,
        _: &str,
        _: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct NoopTokenService;

#[async_trait]
impl TokenService for NoopTokenService {
    async fn generate(
        &self,
        _: domain::UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(String::new())
    }

    async fn verify(
        &self,
        _: &str,
    ) -> Result<domain::UserId, Box<dyn std::error::Error + Send + Sync>> {
        Ok(domain::UserId::from_uuid(uuid::Uuid::new_v4()))
    }
}

fn build_test_app() -> axum::Router {
    let state = AppState::new(
        NoopMonitorRepository,
        MockCheckRepository,
        NoopIncidentRepository,
        UuidIdGenerator::new(),
    );

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
    );

    build_router(state, auth_state)
}

#[tokio::test]
async fn list_checks_returns_ok() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/checks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_monitor_checks_returns_ok() {
    let app = build_test_app();
    let monitor_id = uuid::Uuid::new_v4();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/monitors/{}/checks", monitor_id))
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
