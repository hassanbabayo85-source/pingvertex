use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{CheckRepository, DiagnosticReportRepository, IncidentRepository};
use application::ports::{PasswordHasher, TokenService, UserRepository};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
use domain::User;
use domain::{Check, DiagnosticReport, HttpMethod, Incident};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryTenantMembershipRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, NoopNotificationService, UuidIdGenerator,
};
use serde_json::{json, Value};
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
        InMemoryTenantMembershipRepository::new(),
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
        "interval_seconds": 60,
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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
        user_id: domain::UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(user_id.0.to_string())
    }

    async fn verify(
        &self,
        token: &str,
    ) -> Result<domain::UserId, Box<dyn std::error::Error + Send + Sync>> {
        let user_id = uuid::Uuid::parse_str(token)?;
        Ok(domain::UserId(user_id))
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
        "interval_seconds": 60,
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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
                        "https://example.com",
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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
                        "https://example.com",
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

    // NOTE: the API currently maps every domain validation error in
    // update_monitor through ApiError::internal_with_source, which returns
    // 500 (see crates/api/src/handlers/update_monitor.rs). This test
    // documents that EXISTING behavior for a missing tcp_port on kind
    // switch; it is not asserting that 500 is the ideal status code for a
    // client validation error, only that the request is rejected and the
    // monitor is not silently left in a broken kind=Tcp/tcp_port=null state.
    assert_eq!(put_response.status(), StatusCode::INTERNAL_SERVER_ERROR);

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
    let app = build_test_app();
    let user_id = Uuid::new_v4();

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
