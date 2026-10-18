//! Integration tests for external monitor list pagination.
//!
//! Validates:
//! 1. Default page size (limit absent).
//! 2. Explicit limit clamps.
//! 3. Malformed cursor is ignored.
//! 4. Walking through multiple pages works.
//! 5. Output includes x-next-cursor when there is more data, and omits it at the end.
//! 6. Tenant isolation is maintained.

use api::{
    handlers::list_monitors_via_api_key, middleware::api_key_auth::ApiKeyPrincipal, state::AppState,
};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, MonitorRepository,
    SecretCipher, SecretCipherError,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::get as router_get,
};
use chrono::Utc;
use domain::shared::ids::{ApiKeyId, MonitorId, TenantId, UserId};
use domain::{
    CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url,
};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, UuidIdGenerator,
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

// --- Test Mocks ---

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
#[async_trait::async_trait]
impl CheckRepository for NoopCheckRepository {
    async fn save(
        &self,
        _: &domain::Check,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::shared::ids::CheckId,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<domain::Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn list_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
    ) -> Result<Vec<domain::Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    async fn list_checks(
        &self,
        _: u32,
        _: u32,
        _: domain::shared::ids::TenantId,
    ) -> Result<Vec<domain::Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    async fn delete_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        Ok(0)
    }
}

struct NoopDiagnosticReportRepository;
#[async_trait::async_trait]
impl DiagnosticReportRepository for NoopDiagnosticReportRepository {
    async fn save(
        &self,
        _: &domain::DiagnosticReport,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_check_id(
        &self,
        _: domain::shared::ids::CheckId,
    ) -> Result<Option<domain::DiagnosticReport>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

struct NoopIncidentRepository;
#[async_trait::async_trait]
impl IncidentRepository for NoopIncidentRepository {
    async fn save(
        &self,
        _: &domain::Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(
        &self,
        _: &domain::Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::shared::ids::IncidentId,
    ) -> Result<Option<domain::Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_open_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
    ) -> Result<Option<domain::Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn list_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
    ) -> Result<Vec<domain::Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

// --- Setup ---

fn create_monitor(user_id: UserId, tenant_id: TenantId, index: u16) -> domain::Monitor {
    let builder = MonitorBuilder::new(MonitorId::new(), user_id, tenant_id)
        .name(MonitorName::new(format!("Monitor {}", index)).unwrap())
        .url(Url::new("http://example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap());

    let created_at = Utc::now() + chrono::Duration::milliseconds(index as i64);

    let mut monitor = builder.build();
    monitor.created_at = created_at;
    monitor
}

async fn mock_api_key_auth(mut request: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    // We isolate the borrow of request.headers() inside this block
    // so we can mutate `request` safely afterward.
    let tenant_uuid = {
        let tenant_id_str = request
            .headers()
            .get("x-mock-tenant-id")
            .unwrap()
            .to_str()
            .unwrap();
        uuid::Uuid::parse_str(tenant_id_str).unwrap()
    };

    let user_id = UserId::new();

    request.extensions_mut().insert(ApiKeyPrincipal {
        user_id,
        tenant_id: TenantId(tenant_uuid),
        api_key_id: ApiKeyId::new(),
    });

    Ok(next.run(request).await)
}

async fn build_test_app() -> (axum::Router, TenantId) {
    let monitor_repository = InMemoryMonitorRepository::new();
    let tenant_id = TenantId::new();
    let user_id = UserId::new();

    for i in 1..=15 {
        let monitor = create_monitor(user_id, tenant_id, i);
        monitor_repository.save(&monitor).await.unwrap();
    }

    let other_tenant_id = TenantId::new();
    for i in 1..=5 {
        let monitor = create_monitor(user_id, other_tenant_id, i);
        monitor_repository.save(&monitor).await.unwrap();
    }

    let app_state = AppState::new(
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

    let app = axum::Router::new()
        .route(
            "/api/v1/external/monitors",
            router_get(list_monitors_via_api_key),
        )
        .route_layer(axum::middleware::from_fn(mock_api_key_auth))
        .with_state(app_state);

    (app, tenant_id)
}

async fn get(app: &axum::Router, uri: &str, tenant_id: TenantId) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("x-mock-tenant-id", tenant_id.0.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// --- Tests ---

#[tokio::test]
async fn external_monitor_list_returns_default_page() {
    let (app, tenant) = build_test_app().await;

    let resp = get(&app, "/api/v1/external/monitors", tenant).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let next_cursor = resp.headers().get("x-next-cursor");
    assert!(
        next_cursor.is_none(),
        "Default max page size is 100, so all 15 should fit on page 1"
    );

    let body = read_json(resp).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 15);
}

#[tokio::test]
async fn external_monitor_list_respects_limit_and_returns_cursor() {
    let (app, tenant) = build_test_app().await;

    let resp = get(&app, "/api/v1/external/monitors?limit=5", tenant).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let next_cursor = resp.headers().get("x-next-cursor");
    assert!(
        next_cursor.is_some(),
        "Expected x-next-cursor header to be present"
    );
    let cursor_str = next_cursor.unwrap().to_str().unwrap().to_string();

    let body = read_json(resp).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 5);

    // Fetch page 2
    let resp2 = get(
        &app,
        &format!("/api/v1/external/monitors?limit=5&cursor={}", cursor_str),
        tenant,
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::OK);

    let next_cursor2 = resp2.headers().get("x-next-cursor");
    assert!(next_cursor2.is_some());

    let body2 = read_json(resp2).await;
    let arr2 = body2.as_array().unwrap();
    assert_eq!(arr2.len(), 5);

    // First element of page 2 should be different from first element of page 1
    assert_ne!(arr[0]["id"], arr2[0]["id"]);
}

#[tokio::test]
async fn external_monitor_list_clamps_oversized_limit() {
    let (app, tenant) = build_test_app().await;

    let resp = get(&app, "/api/v1/external/monitors?limit=1000", tenant).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = read_json(resp).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 15); // Clamped to max limit (100) or total available (15)
}

#[tokio::test]
async fn external_monitor_list_ignores_malformed_cursor() {
    let (app, tenant) = build_test_app().await;

    // A garbage cursor should start from page 1 safely.
    let resp = get(
        &app,
        "/api/v1/external/monitors?limit=5&cursor=garbage_cursor_xyz",
        tenant,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = read_json(resp).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 5);
}

#[tokio::test]
async fn external_monitor_list_maintains_tenant_isolation() {
    let (app, tenant) = build_test_app().await;

    let resp = get(&app, "/api/v1/external/monitors?limit=100", tenant).await;
    let body = read_json(resp).await;
    let arr = body.as_array().unwrap();

    // There are 20 monitors total in the DB, but only 15 belong to this API Key's tenant.
    assert_eq!(arr.len(), 15);
}
