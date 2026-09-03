use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher, SecretCipher,
    SecretCipherError, StatusPageRepository, TokenService, UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::entities::{CreateStatusPage, StatusPage};
use domain::shared::ids::{CheckId, IncidentId, MonitorId, StatusPageId, TenantId, UserId};
use domain::value_objects::{CustomDomain, Subdomain};
use domain::{Check, DiagnosticReport, Incident, User};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryLogoStorage,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, InMemoryRegistrationTransaction, InMemoryStatusPageRepository,
    InMemoryTenantMembershipRepository, InMemoryTenantRepository, NoopNotificationService,
    UuidIdGenerator,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// NOTE: this file intentionally defines its own local Noop scaffolding
// rather than sharing it with `status_pages.rs`, following the same
// precedent already used across `crates/api/tests/*.rs` (see B32's
// HANDOVER.md "Deviations" section) — it avoids reproducing (and risking
// a transcription error in) the large existing test file.

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
        id: &UserId,
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
    async fn delete(&self, _: &UserId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        user_id: UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(user_id.0.to_string())
    }
    async fn verify(
        &self,
        token: &str,
    ) -> Result<application::ports::VerifiedToken, Box<dyn std::error::Error + Send + Sync>> {
        let user_id = Uuid::parse_str(token)?;
        Ok(application::ports::VerifiedToken {
            user_id: UserId(user_id),
            issued_at: 0,
        })
    }
}

fn build_test_app_with_status_repo(status_page_repo: InMemoryStatusPageRepository) -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();
    let mut state = AppState::new(
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
    state.status_page_repository = Arc::new(status_page_repo);
    state.logo_storage = Arc::new(InMemoryLogoStorage::new());
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

fn build_test_app() -> axum::Router {
    build_test_app_with_status_repo(InMemoryStatusPageRepository::new())
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn status_page_payload(name: &str, subdomain: &str, monitor_ids: Vec<String>) -> Value {
    json!({
        "name": name,
        "subdomain": subdomain,
        "monitor_ids": monitor_ids,
    })
}

/// Creates a status page via the real HTTP endpoint and returns its id.
async fn create_status_page_via_api(
    app: &axum::Router,
    user_id: Uuid,
    name: &str,
    subdomain: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    status_page_payload(name, subdomain, vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn remove_custom_domain_endpoint_clears_verified_domain() {
    let status_page_repo = InMemoryStatusPageRepository::new();
    let seed_repo = status_page_repo.clone();
    let app = build_test_app_with_status_repo(status_page_repo);
    let user_id = Uuid::new_v4();

    let page_id = create_status_page_via_api(&app, user_id, "Acme Status", "acme").await;

    let request_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/status-pages/{page_id}/custom-domain"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    json!({ "domain": "status.acme.com" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(request_response.status(), StatusCode::OK);

    // Mark verified directly in the repo (bypassing the real DNS sweep,
    // which is exercised separately by `CustomDomainVerificationWorker`)
    // so we can exercise removal of an already-verified domain end to end.
    let status_page_id = StatusPageId(Uuid::parse_str(&page_id).unwrap());
    let mut page = seed_repo.find_by_id(status_page_id).await.unwrap().unwrap();
    page.mark_custom_domain_verified().unwrap();
    seed_repo.update(&page).await.unwrap();

    let remove_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/status-pages/{page_id}/custom-domain"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);

    let body = read_json(remove_response).await;
    assert!(body["custom_domain"].is_null());
    assert_eq!(body["custom_domain_verified"], false);

    // The verified-domain host-routing lookup must no longer find it —
    // this is the security-relevant part of cleanup: it stops serving
    // vhost traffic for a domain the tenant no longer owns.
    let route_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .header("host", "status.acme.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(route_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn custom_domain_routing_rejects_spoofed_host_for_unclaimed_domain() {
    // No status page ever claims this domain at all — a bare Host header
    // spoofing attempt against a domain nobody owns must fall through to
    // normal routing, never leak a status page.
    let app = build_test_app();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .header("host", "totally-unclaimed-domain.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn custom_domain_routing_does_not_match_similar_but_different_domain() {
    // A verified domain "status.example.com" exists. Spoofing attempts
    // using similar-looking but distinct hostnames must NOT match — this
    // proves the routing lookup is an exact string match, not a
    // suffix/substring match a crafted Host header could exploit.
    let status_page_repo = InMemoryStatusPageRepository::new();
    let seed_repo = status_page_repo.clone();

    let mut page = StatusPage::create(CreateStatusPage {
        id: StatusPageId::new(),
        user_id: UserId::new(),
        tenant_id: TenantId::new(),
        name: "Real Status Page".to_string(),
        subdomain: Subdomain::new("real").unwrap(),
        monitor_ids: vec![],
    })
    .unwrap();

    page.request_custom_domain(CustomDomain::new("status.example.com").unwrap());
    page.mark_custom_domain_verified().unwrap();
    seed_repo.save(&page).await.unwrap();

    let app = build_test_app_with_status_repo(status_page_repo);

    for spoofed_host in [
        "evil-status.example.com",
        "status.example.com.evil.net",
        "notstatus.example.com",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/")
                    .header("host", spoofed_host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "spoofed host {spoofed_host} must not match the real verified domain"
        );
    }
}
