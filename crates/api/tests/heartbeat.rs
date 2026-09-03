use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{CheckRepository, DiagnosticReportRepository, IncidentRepository};
use application::ports::{
    PasswordHasher, SecretCipher, SecretCipherError, TokenService, UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::User;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId};
use domain::{Check, DiagnosticReport, Incident};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, NoopNotificationService, UuidIdGenerator,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

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

struct NoopCheckRepository;

#[async_trait]
impl CheckRepository for NoopCheckRepository {
    async fn save(&self, _: &Check) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _: CheckId,
        _: chrono::DateTime<chrono::Utc>,
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
        _: TenantId,
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
        id: &domain::UserId,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        // jwt_auth middleware reloads the user on every request to check
        // `status` (P0 fix: suspended/deleted accounts must lose access
        // immediately). This stub hands back an Active user for whichever
        // id the middleware asks about.
        let now = chrono::Utc::now();
        let user = domain::UserBuilder::new()
            .id(*id)
            .username(
                domain::Username::new(format!("u{}", &id.0.simple().to_string()[..8])).unwrap(),
            )
            .email(domain::Email::new(format!("user-{}@example.test", id.0.simple())).unwrap())
            .password_hash(domain::PasswordHash::new("test-hash-not-checked").unwrap())
            .role(domain::UserRole::User)
            .status(domain::UserStatus::Active)
            .email_verified_at(Some(now))
            .created_at(now)
            .updated_at(now)
            .build()
            .unwrap();
        Ok(Some(user))
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
        user_id: domain::UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(user_id.0.to_string())
    }

    async fn verify(
        &self,
        token: &str,
    ) -> Result<application::ports::VerifiedToken, Box<dyn std::error::Error + Send + Sync>> {
        let user_id = uuid::Uuid::parse_str(token)?;
        Ok(application::ports::VerifiedToken {
            user_id: domain::UserId(user_id),
            issued_at: 0,
        })
    }
}

fn build_test_app() -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();

    let state = AppState::new(
        monitor_repository.clone(),
        NoopCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repository),
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
        InMemoryTenantRepository::new(),
        InMemoryTenantMembershipRepository::new(),
        InMemoryRegistrationTransaction::new(),
    );
    build_test_router(state, auth_state)
}

fn heartbeat_monitor_payload(
    user_id: Uuid,
    name: &str,
    heartbeat_interval_seconds: u32,
    heartbeat_grace_period_seconds: u32,
) -> Value {
    json!({
        "user_id": user_id.to_string(),
        "name": name,
        // `url` is still required by MonitorWriteRequest/CreateMonitorRequest
        // regardless of `kind` in the current implementation (not yet made
        // optional per-kind) — a harmless placeholder is enough here.
        "url": "https://example.com",
        "method": "GET",
        "expected_status": 200,
        "interval_seconds": 120,
        "timeout_seconds": 10,
        "assertions": [],
        "kind": "Heartbeat",
        "heartbeat_interval_seconds": heartbeat_interval_seconds,
        "heartbeat_grace_period_seconds": heartbeat_grace_period_seconds
    })
}

fn http_monitor_payload(user_id: Uuid, name: &str) -> Value {
    json!({
        "user_id": user_id.to_string(),
        "name": name,
        "url": "https://example.com",
        "method": "GET",
        "expected_status": 200,
        "interval_seconds": 120,
        "timeout_seconds": 10,
        "assertions": []
    })
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_heartbeat_monitor(app: &axum::Router, user_id: Uuid, name: &str) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    heartbeat_monitor_payload(user_id, name, 300, 60).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    if response.status() != StatusCode::CREATED {
        let status = response.status();

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        panic!(
            "expected 201 CREATED, got {}: {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }
    read_json(response).await
}

#[tokio::test]
async fn create_heartbeat_monitor_returns_token_exactly_once() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();

    let created = create_heartbeat_monitor(&app, user_id, "Nightly Backup Job").await;

    assert_eq!(created["kind"], "Heartbeat");
    let token = created["heartbeat_token"]
        .as_str()
        .expect("heartbeat_token must be a non-null string in the create response");
    assert!(!token.is_empty());
}

#[tokio::test]
async fn http_monitor_create_response_has_null_heartbeat_token() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    http_monitor_payload(user_id, "Regular Http Monitor").to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    if response.status() != StatusCode::CREATED {
        let status = response.status();

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        panic!(
            "expected 201 CREATED, got {}: {}",
            status,
            String::from_utf8_lossy(&bytes)
        );
    }
    let created = read_json(response).await;

    assert_eq!(created["kind"], "Http");
    assert!(created["heartbeat_token"].is_null());
}

#[tokio::test]
async fn heartbeat_ping_updates_last_heartbeat_at_and_returns_ok() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();

    let created = create_heartbeat_monitor(&app, user_id, "Nightly Backup Job").await;
    let monitor_id = created["id"].as_str().unwrap();
    let token = created["heartbeat_token"].as_str().unwrap();

    // Before the ping, last_heartbeat_at must be null.
    let before_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let before = read_json(before_response).await;
    assert!(before["last_heartbeat_at"].is_null());

    let ping_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/public/heartbeat/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(ping_response.status(), StatusCode::OK);
    let ping_body = read_json(ping_response).await;
    assert_eq!(ping_body["status"], "ok");

    // After the ping, last_heartbeat_at must be set.
    let after_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let after = read_json(after_response).await;
    assert!(!after["last_heartbeat_at"].is_null());
}

#[tokio::test]
async fn heartbeat_ping_with_unknown_token_returns_not_found() {
    let app = build_test_app();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/public/heartbeat/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_and_list_monitor_endpoints_never_expose_heartbeat_token() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();

    let created = create_heartbeat_monitor(&app, user_id, "Nightly Backup Job").await;
    let monitor_id = created["id"].as_str().unwrap();

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let get_body = read_json(get_response).await;
    assert!(
        get_body.get("heartbeat_token").is_none(),
        "GET /monitors/{{id}} must never expose heartbeat_token"
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list_body = read_json(list_response).await;
    for monitor in list_body.as_array().unwrap() {
        assert!(
            monitor.get("heartbeat_token").is_none(),
            "GET /monitors (list) must never expose heartbeat_token"
        );
    }
}

#[tokio::test]
async fn heartbeat_ping_is_rate_limited_per_token() {
    // Must match HEARTBEAT_RATE_LIMIT_MAX_REQUESTS in
    // crates/api/src/handlers/heartbeat.rs.
    const MAX_REQUESTS_PER_WINDOW: usize = 6;

    let app = build_test_app();
    let user_id = Uuid::new_v4();

    let created = create_heartbeat_monitor(&app, user_id, "Nightly Backup Job").await;
    let token = created["heartbeat_token"].as_str().unwrap();

    for attempt in 0..MAX_REQUESTS_PER_WINDOW {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/public/heartbeat/{token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} within the limit should succeed"
        );
    }

    let over_limit_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/public/heartbeat/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(over_limit_response.status(), StatusCode::TOO_MANY_REQUESTS);
}
