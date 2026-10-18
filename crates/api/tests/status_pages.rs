use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher,
    StatusPageRepository, TokenService, UserRepository,
};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::entities::{CreateStatusPage, StatusPage};
use domain::shared::ids::TenantId;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, StatusPageId, UserId};
use domain::value_objects::{CustomDomain, Subdomain};
use domain::{Check, DiagnosticReport, Incident, User};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryLogoStorage, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    InMemoryStatusPageRepository, NoopNotificationService, UuidIdGenerator,
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

fn build_test_app_with_status_repo(
    status_page_repo: InMemoryStatusPageRepository,
    user_ids: &[Uuid],
) -> axum::Router {
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
    )
    .with_entitlement_resolver(infra::InMemoryEntitlementResolver::default_enterprise());

    // AppState::new installs a NoopStatusPageRepository. Replace it with a
    // real in-memory repo so status-page routes and custom-domain routing
    // can actually resolve data in these tests.
    state.status_page_repository = Arc::new(status_page_repo);
    state.logo_storage = Arc::new(InMemoryLogoStorage::new());

    let tenant_repository = InMemoryTenantRepository::new();
    let tenant_membership_repository = InMemoryTenantMembershipRepository::new();

    for user_id in user_ids {
        let tenant_id = tenant_repository.seed_for_user(UserId(*user_id));
        tenant_membership_repository.seed_owner(tenant_id, UserId(*user_id));
    }

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        tenant_repository,
        tenant_membership_repository,
        InMemoryRegistrationTransaction::new(),
    );

    build_test_router(state, auth_state)
}

fn build_test_app(user_ids: &[Uuid]) -> axum::Router {
    build_test_app_with_status_repo(InMemoryStatusPageRepository::new(), user_ids)
}

fn status_page_payload(name: &str, subdomain: &str, monitor_ids: Vec<String>) -> Value {
    json!({
        "name": name,
        "subdomain": subdomain,
        "monitor_ids": monitor_ids,
    })
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn create_and_list_status_pages_endpoint_lists_created_pages() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(&[user_id]);

    for (name, subdomain) in [("Acme", "acme"), ("Beta", "beta")] {
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
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/status-pages")
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = read_json(response).await;
    let status_pages = body.as_array().unwrap();

    assert_eq!(status_pages.len(), 2);

    let names: Vec<_> = status_pages
        .iter()
        .map(|page| page["name"].as_str().unwrap().to_string())
        .collect();

    assert!(names.contains(&"Acme".to_string()));
    assert!(names.contains(&"Beta".to_string()));
}

#[tokio::test]
async fn status_page_ownership_isolation_prevents_update_and_delete() {
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let app = build_test_app(&[user_a, user_b]);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_a))
                .body(Body::from(
                    status_page_payload("Isolation", "isolation", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    let status_page_id = created["id"].as_str().unwrap();

    // User B cannot update A's status page.
    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/status-pages/{status_page_id}"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_b))
                .body(Body::from(
                    status_page_payload("Hijacked", "isolation", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::NOT_FOUND);

    // User B cannot delete A's status page.
    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/status-pages/{status_page_id}"))
                .header("authorization", auth_header(user_b))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);

    // User B sees nothing in their list.
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/status-pages")
                .header("authorization", auth_header(user_b))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let body = read_json(list_response).await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn public_status_page_endpoint_serves_page_without_owner_fields() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(&[user_id]);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    status_page_payload("Public Page", "pubpage", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/public/status/pubpage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = read_json(response).await;
    assert_eq!(body["name"], "Public Page");
    assert!(body.get("user_id").is_none());
    assert_eq!(body["monitors"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn subdomain_uniqueness_is_enforced_at_http_layer() {
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let app = build_test_app(&[user_a, user_b]);

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_a))
                .body(Body::from(
                    status_page_payload("First", "shared", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(first_response.status(), StatusCode::CREATED);

    let second_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_b))
                .body(Body::from(
                    status_page_payload("Second", "shared", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(second_response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn custom_domain_routing_serves_verified_domain_without_auth() {
    let status_page_repo = InMemoryStatusPageRepository::new();
    let seed_repo = status_page_repo.clone();

    let mut page = StatusPage::create(CreateStatusPage {
        id: StatusPageId::new(),
        user_id: UserId::new(),
        tenant_id: TenantId::new(),
        name: "Custom Domain Page".to_string(),
        subdomain: Subdomain::new("custom").unwrap(),
        monitor_ids: vec![],
    })
    .unwrap();

    page.request_custom_domain(CustomDomain::new("status.example.com").unwrap());
    page.mark_custom_domain_verified().unwrap();

    seed_repo.save(&page).await.unwrap();
    let user_id = Uuid::new_v4();

    let app = build_test_app_with_status_repo(status_page_repo, &[user_id]);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .header("host", "status.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = read_json(response).await;
    assert_eq!(body["name"], "Custom Domain Page");
    assert!(body.get("user_id").is_none());
}

#[tokio::test]
async fn custom_domain_routing_does_not_serve_pending_domain() {
    let status_page_repo = InMemoryStatusPageRepository::new();
    let seed_repo = status_page_repo.clone();

    let mut page = StatusPage::create(CreateStatusPage {
        id: StatusPageId::new(),
        user_id: UserId::new(),
        tenant_id: TenantId::new(),
        name: "Pending Domain Page".to_string(),
        subdomain: Subdomain::new("pending").unwrap(),
        monitor_ids: vec![],
    })
    .unwrap();

    page.request_custom_domain(CustomDomain::new("pending.example.com").unwrap());
    // Deliberately NOT calling mark_custom_domain_verified().

    let user_id = Uuid::new_v4();
    seed_repo.save(&page).await.unwrap();

    let app = build_test_app_with_status_repo(status_page_repo, &[user_id]);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .header("host", "pending.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

fn multipart_logo_body(boundary: &str, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"logo\"; filename=\"logo.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[tokio::test]
async fn upload_status_page_logo_endpoint_stores_logo_and_updates_page() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(&[user_id]);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    status_page_payload("Logo Page", "logopage", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    let status_page_id = created["id"].as_str().unwrap();

    let boundary = "test-boundary";
    let png_signature = b"\x89PNG\r\n\x1a\n";
    let multipart_body = multipart_logo_body(boundary, png_signature);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/status-pages/{status_page_id}/logo"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("authorization", auth_header(user_id))
                .body(Body::from(multipart_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    let logo_url = body["logo_url"]
        .as_str()
        .expect("logo_url should be present");
    assert!(logo_url.starts_with("https://cdn.test/logos/inmemory/"));
    assert!(logo_url.ends_with(".png"));
}
