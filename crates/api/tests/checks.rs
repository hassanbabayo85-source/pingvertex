use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{CheckRepository, DiagnosticReportRepository, IncidentRepository};
use application::ports::{PasswordHasher, TokenService, UserRepository};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
use domain::User;
use domain::{Check, DiagnosticReport, Incident};
use infra::{
    InMemoryApiFlowRepository, InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, NoopNotificationService, UuidIdGenerator,
};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Mock repositories for checks, incidents, and user/auth
// ---------------------------------------------------------------------------

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
        _: UserId,
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
struct NoopDiagnosticReportRepository;

#[async_trait]
impl DiagnosticReportRepository for NoopDiagnosticReportRepository {
    async fn save(
        &self,
        _: &DiagnosticReport,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_check_id(
        &self,
        _: CheckId,
    ) -> Result<Option<DiagnosticReport>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
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

// ---------------------------------------------------------------------------
// No-op auth services – JWT verification returns a fixed UserId so tests
// are consistent across requests.
// ---------------------------------------------------------------------------

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
        // FIX: Tsayayyen UserId (nil UUID) don kowane buƙata ya kasance iri ɗaya.
        // Wannan yana tabbatar da cewa POST da GET a gwaji suna da mai amfani iri ɗaya,
        // don haka monitor ɗin da aka ƙirƙira zai kasance mallakar wannan mai amfanin.
        Ok(domain::UserId::from_uuid(uuid::Uuid::nil()))
    }
}

// ---------------------------------------------------------------------------
// Test helper
// ---------------------------------------------------------------------------

fn build_test_app() -> axum::Router {
    // InMemoryMonitorRepository yana da cikakkiyar aiki na MonitorRepository,
    // gami da find_by_id_for_user() da ownership checks.
    let monitor_repo = InMemoryMonitorRepository::new();

    let state = AppState::new(
        monitor_repo.clone(),
        MockCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
    );

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
    );

    build_test_router(state, auth_state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_checks_returns_ok() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/checks")
                .header("Authorization", "Bearer test-token")
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

    // Fara ƙirƙirar monitor ta hanyar amfani da router ɗin da muka gina.
    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("Authorization", "Bearer test-token")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{
                        "name": "Test Monitor",
                        "url": "https://example.com",
                        "method": "GET",
                        "expected_status": 200,
                        "interval_seconds": 60,
                        "timeout_seconds": 10,
                        "assertions": []
                    }"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);

    // Cire ID daga cikin amsar
    let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let monitor_id = json["id"].as_str().unwrap();

    // Yanzu neman checks na wannan monitor
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
