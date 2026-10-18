use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    ApiKeyRepository, CheckRepository, DiagnosticReportRepository, IncidentRepository,
    PasswordHasher, TenantMembershipRepository, TenantRepository, TokenService, UserRepository,
};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use domain::entities::{ApiKey, CreateApiKey as CreateApiKeyEntity};
use domain::enums::ApiKeyScope;
use domain::shared::ids::{ApiKeyId, CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{Check, DiagnosticReport, Incident, Tenant, TenantMembership, User};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryApiKeyRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, UuidIdGenerator,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Noop / mock dependencies (mirrors crates/api/tests/notifications.rs)
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

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn test_user_id() -> Uuid {
    Uuid::from_u128(0x11111111111111111111111111111111)
}

fn test_tenant_id() -> TenantId {
    TenantId(Uuid::from_u128(0x22222222222222222222222222222222))
}

async fn provision_test_membership(
    tenant_repository: &InMemoryTenantRepository,
    membership_repository: &InMemoryTenantMembershipRepository,
) {
    let user_id = UserId(test_user_id());
    let tenant_id = test_tenant_id();
    let now = Utc::now();

    let tenant = Tenant::new(tenant_id, "API Key Test Tenant", user_id).unwrap();

    tenant_repository.create(&tenant).await.unwrap();

    let membership = TenantMembership {
        tenant_id,
        user_id,
        role: domain::shared::types::Role::Owner,
        invited_at: now,
        joined_at: Some(now),
    };

    membership_repository.create(&membership).await.unwrap();
}

async fn build_test_app_with_repo() -> (axum::Router, InMemoryApiKeyRepository) {
    let monitor_repository = InMemoryMonitorRepository::new();
    let api_key_repo = InMemoryApiKeyRepository::new();

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
    )
    .with_api_key_repository(api_key_repo.clone())
    .with_entitlement_resolver(infra::InMemoryEntitlementResolver::default_enterprise());

    let tenant_repository = InMemoryTenantRepository::new();
    let membership_repository = InMemoryTenantMembershipRepository::new();

    provision_test_membership(&tenant_repository, &membership_repository).await;

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        tenant_repository,
        membership_repository,
        InMemoryRegistrationTransaction::new(),
    );

    (build_test_router(state, auth_state), api_key_repo)
}

async fn build_test_app() -> axum::Router {
    build_test_app_with_repo().await.0
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_key(
    app: &axum::Router,
    user_id: Uuid,
    name: &str,
    scopes: &[&str],
) -> (Uuid, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": name,
                        "scopes": scopes,
                        "expires_at": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    let id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    let raw = body["key"].as_str().unwrap().to_string();
    (id, raw)
}

// ---------------------------------------------------------------------------
// Existing tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_api_key_returns_the_raw_key_exactly_once() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "CI integration",
                        "scopes": ["monitor:read", "monitor:write"],
                        "expires_at": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert!(body["key"].as_str().unwrap().starts_with("pv_"));
    assert_eq!(body["name"], "CI integration");
    assert_eq!(body["scopes"], json!(["monitor:read", "monitor:write"]));
}

#[tokio::test]
async fn create_api_key_rejects_empty_scopes() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"name": "No scopes", "scopes": []}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_api_key_rejects_unknown_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"name": "Bad scope", "scopes": ["monitor:delete"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_api_keys_returns_created_keys_without_the_raw_key_or_hash() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"name": "Dashboard", "scopes": ["status_page:read"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let list_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let body = body_json(list_response).await;
    let keys = body.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["name"], "Dashboard");
    assert!(keys[0].get("key").is_none());
    assert!(keys[0].get("hashed_key").is_none());
    assert!(keys[0]["key_prefix"].as_str().unwrap().starts_with("pv_"));
}

#[tokio::test]
async fn delete_api_key_removes_it_from_the_list() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"name": "Throwaway", "scopes": ["monitor:read"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let created = body_json(create_response).await;
    let key_id = created["id"].as_str().unwrap();

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/api-keys/{key_id}"))
                .header("Authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let list_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(list_response).await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn delete_api_key_is_not_found_for_an_unknown_id() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/api-keys/{}", Uuid::new_v4()))
                .header("Authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn external_endpoint_rejects_missing_api_key() {
    let app = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/monitors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn external_endpoint_accepts_a_key_with_the_right_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    let (_, raw_key) = create_key(&app, user_id, "Reader", &["monitor:read"]).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/monitors")
                .header("X-API-Key", raw_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn external_endpoint_rejects_a_key_missing_the_required_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    // Key only has status_page:read — NOT monitor:read.
    let (_, raw_key) = create_key(&app, user_id, "Wrong scope", &["status_page:read"]).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/monitors")
                .header("X-API-Key", raw_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn external_endpoint_rejects_a_malformed_or_unknown_key() {
    let app = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/monitors")
                .header("X-API-Key", "not-a-real-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Scope enforcement per endpoint, revoked/expired keys
// ---------------------------------------------------------------------------

#[tokio::test]
async fn external_checks_requires_monitor_read_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    // Right scope: monitor:read -> 200 (empty list)
    let (_, monitor_read_key) = create_key(&app, user_id, "Checks OK", &["monitor:read"]).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/checks")
                .header("X-API-Key", monitor_read_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Wrong scope -> 403
    let (_, wrong_key) = create_key(&app, user_id, "Checks Wrong", &["status_page:read"]).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/checks")
                .header("X-API-Key", wrong_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn external_list_incidents_requires_incident_read_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();
    let monitor_id = Uuid::new_v4();

    // Right scope: incident:read -> 404 (monitor doesn't exist in this test)
    let (_, read_key) = create_key(&app, user_id, "Incidents Read", &["incident:read"]).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/external/incidents/{monitor_id}"))
                .header("X-API-Key", read_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Wrong scope -> 403
    let (_, wrong_key) = create_key(&app, user_id, "Incidents Wrong", &["status_page:read"]).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/external/incidents/{monitor_id}"))
                .header("X-API-Key", wrong_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn external_acknowledge_incident_requires_incident_write_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();
    let incident_id = Uuid::new_v4();

    // Right scope: incident:write -> 404 (incident doesn't exist)
    let (_, write_key) = create_key(&app, user_id, "Incidents Write", &["incident:write"]).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/external/incidents/{incident_id}/acknowledge"
                ))
                .header("X-API-Key", write_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Wrong scope (incident:read) -> 403
    let (_, read_key) = create_key(&app, user_id, "Incidents Read", &["incident:read"]).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/external/incidents/{incident_id}/acknowledge"
                ))
                .header("X-API-Key", read_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn external_list_status_pages_requires_status_page_read_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();

    // Right scope -> 200 (empty list)
    let (_, read_key) = create_key(&app, user_id, "SP Read", &["status_page:read"]).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/status-pages")
                .header("X-API-Key", read_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Wrong scope -> 403
    let (_, wrong_key) = create_key(&app, user_id, "SP Wrong", &["monitor:read"]).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/status-pages")
                .header("X-API-Key", wrong_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn external_update_status_page_requires_status_page_write_scope() {
    let app = build_test_app().await;
    let user_id = test_user_id();
    let status_page_id = Uuid::new_v4();

    // Right scope -> 404 (status page doesn't exist)
    let (_, write_key) = create_key(&app, user_id, "SP Write", &["status_page:write"]).await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/external/status-pages/{status_page_id}"))
                .header("X-API-Key", write_key)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "New Name",
                        "monitor_ids": [],
                        "logo_url": null,
                        "primary_color": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Wrong scope (status_page:read) -> 403
    let (_, read_key) = create_key(&app, user_id, "SP Read", &["status_page:read"]).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/external/status-pages/{status_page_id}"))
                .header("X-API-Key", read_key)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "New Name",
                        "monitor_ids": [],
                        "logo_url": null,
                        "primary_color": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn revoked_api_key_is_rejected() {
    let (app, _repo) = build_test_app_with_repo().await;
    let user_id = test_user_id();

    let (key_id, raw_key) = create_key(&app, user_id, "Revoke Me", &["monitor:read"]).await;

    // Revoke via the normal API
    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/api-keys/{key_id}"))
                .header("Authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    // Now the raw key must be rejected
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/monitors")
                .header("X-API-Key", raw_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_api_key_is_rejected() {
    let (app, repo) = build_test_app_with_repo().await;

    // Directly insert an already-expired key into the repository.
    let raw_key = "pv_expired_secret";
    let api_key = ApiKey::create(CreateApiKeyEntity {
        id: ApiKeyId::new(),
        tenant_id: TenantId::new(),
        user_id: UserId::new(),
        name: "Expired Key".to_string(),
        key_prefix: "pv_expired".to_string(),
        hashed_key: "unused-hash".to_string(),
        scopes: vec![ApiKeyScope::MonitorRead],
        expires_at: Some(Utc::now() - Duration::seconds(1)),
    });
    repo.create(&api_key).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/external/monitors")
                .header("X-API-Key", raw_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
