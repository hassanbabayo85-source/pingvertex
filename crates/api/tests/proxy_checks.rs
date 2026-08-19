use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{CheckRepository, DiagnosticReportRepository, IncidentRepository};
use application::ports::{PasswordHasher, TokenService, UserRepository};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header::CONTENT_TYPE, Request, StatusCode};
use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
use domain::User;
use domain::{Check, DiagnosticReport, Incident};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryTenantMembershipRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, NoopNotificationService, UuidIdGenerator,
};
use std::sync::Arc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Mock repositories for checks, incidents, and user/auth
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct NoopSecretCipher;

impl SecretCipher for NoopSecretCipher {
    fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(plaintext.to_vec())
    }
    fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(ciphertext.to_vec())
    }
}
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
// No-op auth services
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
        Ok(domain::UserId::from_uuid(uuid::Uuid::nil()))
    }
}

// ---------------------------------------------------------------------------
// Test helper
// ---------------------------------------------------------------------------

fn build_test_app() -> axum::Router {
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
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    );

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        InMemoryTenantMembershipRepository::new(),
    );

    build_test_router(state, auth_state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_proxy_check_job_with_csv_returns_created() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .header(CONTENT_TYPE, "text/csv")
                .body(Body::from("1.2.3.4:8080\n"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "Pending");
    assert_eq!(json["total_count"], 1);
    assert_eq!(json["checked_count"], 0);
    assert!(json["completed_at"].is_null());
    assert!(json["created_at"].is_string());
    assert!(json["id"].is_string());
    assert!(json["user_id"].is_string());
}

#[tokio::test]
async fn create_proxy_check_job_with_json_returns_created() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"[{"host":"1.2.3.4","port":8080,"scheme":"Http"}]"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "Pending");
    assert_eq!(json["total_count"], 1);
    assert_eq!(json["checked_count"], 0);
    assert!(json["completed_at"].is_null());
}

#[tokio::test]
async fn create_proxy_check_job_missing_content_type_returns_bad_request() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .body(Body::from("1.2.3.4:8080"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_proxy_check_job_with_empty_csv_returns_bad_request() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .header(CONTENT_TYPE, "text/csv")
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_proxy_check_job_with_invalid_content_type_returns_bad_request() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .header(CONTENT_TYPE, "text/plain")
                .body(Body::from("1.2.3.4:8080"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_proxy_check_job_without_auth_returns_unauthorized() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header(CONTENT_TYPE, "text/csv")
                .body(Body::from("1.2.3.4:8080"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// GET /proxy-checks/{id} tests
// ---------------------------------------------------------------------------

async fn submit_proxy_job(app: axum::Router, body: &str) -> (axum::Router, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .header(CONTENT_TYPE, "text/csv")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let id = json["id"].as_str().unwrap().to_string();
    (app, id)
}

#[tokio::test]
async fn get_proxy_check_job_returns_detail() {
    let app = build_test_app();
    let (app, id) = submit_proxy_job(app, "1.2.3.4:8080\n").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/proxy-checks/{id}"))
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["id"], id);
    assert_eq!(json["status"], "Pending");
    assert_eq!(json["total_count"], 1);
    assert_eq!(json["checked_count"], 0);
    assert!(json["completed_at"].is_null());
    assert!(json["entries"].is_array());
    assert_eq!(json["entries"][0]["host"], "1.2.3.4");
    assert_eq!(json["entries"][0]["port"], 8080);
    assert_eq!(json["entries"][0]["scheme"], "Http");
    assert!(json["entries"][0]["result"].is_null());
    assert!(json["entries"][0].get("credentials").is_none());
}

#[tokio::test]
async fn get_proxy_check_job_invalid_uuid_returns_bad_request() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/proxy-checks/not-a-uuid")
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_proxy_check_job_not_found_returns_not_found() {
    let app = build_test_app();

    let id = domain::shared::ids::ProxyCheckJobId::new().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/proxy-checks/{id}"))
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn submit_proxy_check_job_enforces_daily_quota() {
    let app = build_test_app();

    let body = "1.2.3.4:8080\n";

    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/proxy-checks")
                    .header("Authorization", "Bearer test-token")
                    .header(CONTENT_TYPE, "text/csv")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/proxy-checks")
                .header("Authorization", "Bearer test-token")
                .header(CONTENT_TYPE, "text/csv")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
