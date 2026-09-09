use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{CheckRepository, DiagnosticReportRepository, IncidentRepository};
use application::ports::{PasswordHasher, TokenService, UserRepository};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::User;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId};
use domain::{Check, DiagnosticReport, HttpMethod, Incident};
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

fn build_test_app(user_id: Uuid) -> axum::Router {
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

    let tenant_repository = InMemoryTenantRepository::new();
    let tenant_membership_repository = InMemoryTenantMembershipRepository::new();

    let tenant_id = tenant_repository.seed_for_user(domain::UserId(user_id));
    tenant_membership_repository.seed_owner(tenant_id, domain::UserId(user_id));

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

fn monitor_payload(user_id: Uuid, name: &str, url: &str, method: HttpMethod) -> Value {
    json!({
        "user_id": user_id.to_string(),
        "name": name,
        "url": url,
        "method": method,
        "expected_status": 200,
        "interval_seconds": 600,
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

#[tokio::test]
async fn list_monitors_endpoint_lists_created_monitors() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    for name in ["Monitor 1", "Monitor 2"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/monitors")
                    .header("content-type", "application/json")
                    .header("authorization", auth_header(user_id))
                    .body(Body::from(
                        monitor_payload(user_id, name, "https://example.com", HttpMethod::Get)
                            .to_string(),
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
                .uri("/api/v1/monitors")
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = read_json(response).await;
    let monitors = body.as_array().unwrap();

    assert_eq!(monitors.len(), 2);

    let names: Vec<_> = monitors
        .iter()
        .map(|monitor| monitor["name"].as_str().unwrap().to_string())
        .collect();

    assert!(names.contains(&"Monitor 1".to_string()));
    assert!(names.contains(&"Monitor 2".to_string()));
}

#[tokio::test]
async fn get_monitor_endpoint_returns_monitor() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(user_id, "Google", "https://google.com", HttpMethod::Get)
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();

    let response = app
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

    assert_eq!(response.status(), StatusCode::OK);

    let body = read_json(response).await;
    assert_eq!(body["id"], monitor_id);
    assert_eq!(body["name"], "Google");
}

#[tokio::test]
async fn update_monitor_endpoint_updates_monitor() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(user_id, "Google", "https://google.com", HttpMethod::Get)
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(
                        user_id,
                        "Google Search",
                        "https://www.google.com",
                        HttpMethod::Head,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = read_json(response).await;
    assert_eq!(body["id"], monitor_id);
    assert_eq!(body["name"], "Google Search");
    assert_eq!(body["url"], "https://www.google.com/");
}

#[tokio::test]
async fn delete_monitor_endpoint_deletes_monitor() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(user_id, "Google", "https://google.com", HttpMethod::Get)
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

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

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
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

#[allow(clippy::too_many_arguments)]
fn monitor_payload_with_kind(
    user_id: Uuid,
    name: &str,
    url: &str,
    method: HttpMethod,
    kind: &str,
    tcp_port: Option<u16>,
    dns_record_type: Option<&str>,
    dns_expected_value: Option<&str>,
    heartbeat_interval_seconds: Option<u32>,
    heartbeat_grace_period_seconds: Option<u32>,
) -> Value {
    json!({
        "user_id": user_id.to_string(),
        "name": name,
        "url": url,
        "method": method,
        "expected_status": 200,
        "interval_seconds": 600,
        "timeout_seconds": 10,
        "assertions": [],
        "kind": kind,
        "tcp_port": tcp_port,
        "dns_record_type": dns_record_type,
        "dns_expected_value": dns_expected_value,
        "heartbeat_interval_seconds": heartbeat_interval_seconds,
        "heartbeat_grace_period_seconds": heartbeat_grace_period_seconds
    })
}

#[tokio::test]
async fn update_monitor_endpoint_switches_kind_from_http_to_tcp_and_persists_tcp_port() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(user_id, "Db Server", "https://example.com", HttpMethod::Get)
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();
    assert_eq!(created["kind"], "Http");

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload_with_kind(
                        user_id,
                        "Db Server",
                        "db.example.com",
                        HttpMethod::Get,
                        "Tcp",
                        Some(5432),
                        None,
                        None,
                        None,
                        None,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(put_response.status(), StatusCode::OK);
    let updated = read_json(put_response).await;
    assert_eq!(updated["kind"], "Tcp");
    assert_eq!(updated["tcp_port"], 5432);

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
    let fetched = read_json(get_response).await;
    assert_eq!(
        fetched["kind"], "Tcp",
        "the kind switch must survive a fresh GET, not just the PUT response body"
    );
    assert_eq!(fetched["tcp_port"], 5432);
}

#[tokio::test]
async fn update_monitor_endpoint_switching_kind_clears_stale_fields_from_previous_kind() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload_with_kind(
                        user_id,
                        "Db Server",
                        "db.example.com",
                        HttpMethod::Get,
                        "Tcp",
                        Some(5432),
                        None,
                        None,
                        None,
                        None,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();
    assert_eq!(created["tcp_port"], 5432);

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload_with_kind(
                        user_id,
                        "Db Server",
                        "db.example.com",
                        HttpMethod::Get,
                        "Dns",
                        None,
                        Some("A"),
                        Some("1.2.3.4"),
                        None,
                        None,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(put_response.status(), StatusCode::OK);
    let updated = read_json(put_response).await;
    assert_eq!(updated["kind"], "Dns");
    assert_eq!(updated["dns_record_type"], "A");
    assert_eq!(updated["dns_expected_value"], "1.2.3.4");
    assert!(
        updated["tcp_port"].is_null(),
        "switching away from Tcp must clear the stale tcp_port, not leave 5432 dangling"
    );
}

#[tokio::test]
async fn update_monitor_endpoint_switching_to_tcp_without_tcp_port_is_rejected() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(user_id, "Db Server", "https://example.com", HttpMethod::Get)
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload_with_kind(
                        user_id,
                        "Db Server",
                        "db.example.com",
                        HttpMethod::Get,
                        "Tcp",
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Now the API maps domain validation errors to 400 for this case.
    assert_eq!(put_response.status(), StatusCode::BAD_REQUEST);

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
    let fetched = read_json(get_response).await;
    assert_eq!(
        fetched["kind"], "Http",
        "a rejected kind switch must not partially persist — the monitor must stay Http"
    );
}

#[tokio::test]
async fn update_monitor_endpoint_switching_to_heartbeat_sets_kind_and_never_exposes_token() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(
                        user_id,
                        "Nightly Job",
                        "https://example.com",
                        HttpMethod::Get,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/monitors/{monitor_id}"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload_with_kind(
                        user_id,
                        "Nightly Job",
                        "https://example.com",
                        HttpMethod::Get,
                        "Heartbeat",
                        None,
                        None,
                        None,
                        Some(300),
                        Some(60),
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(put_response.status(), StatusCode::OK);
    let updated = read_json(put_response).await;
    assert_eq!(updated["kind"], "Heartbeat");
    assert_eq!(updated["heartbeat_interval_seconds"], 300);
    assert_eq!(updated["heartbeat_grace_period_seconds"], 60);
    assert!(
        updated.get("heartbeat_token").is_none(),
        "PUT must never expose heartbeat_token, same as GET/list"
    );
}

#[tokio::test]
async fn create_monitor_with_tcp_and_url_scheme_is_rejected() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload_with_kind(
                        user_id,
                        "Db Server",
                        "https://example.com",
                        HttpMethod::Get,
                        "Tcp",
                        Some(5432),
                        None,
                        None,
                        None,
                        None,
                    )
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn enable_disable_monitor_endpoints_toggle_enabled_field() {
    let user_id = Uuid::new_v4();
    let app = build_test_app(user_id);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    monitor_payload(user_id, "Google", "https://google.com", HttpMethod::Get)
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let created = read_json(create_response).await;
    let monitor_id = created["id"].as_str().unwrap();
    assert_eq!(created["enabled"], true);

    // Disable
    let disable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/monitors/{monitor_id}/disable"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disable_response.status(), StatusCode::OK);
    let disabled = read_json(disable_response).await;
    assert_eq!(disabled["enabled"], false);

    // Get and verify
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
    let fetched = read_json(get_response).await;
    assert_eq!(fetched["enabled"], false);

    // Enable
    let enable_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/monitors/{monitor_id}/enable"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enable_response.status(), StatusCode::OK);
    let enabled = read_json(enable_response).await;
    assert_eq!(enabled["enabled"], true);
}
