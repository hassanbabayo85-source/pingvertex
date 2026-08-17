use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, NotificationService,
    NotifyResult, PasswordHasher, TokenService, UserRepository,
};
use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use domain::entities::NotificationChannel;
use domain::notification::NotificationEvent;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
use domain::{Check, DiagnosticReport, HttpMethod, Incident, User};
use infra::{
    InMemoryApiFlowRepository, InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, NoopNotificationService, UuidIdGenerator,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Noop / mock dependencies (mirrors crates/api/tests/monitors.rs)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
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

/// A `NotificationService` that always fails, used to prove
/// `POST /notifications/{id}/test` surfaces a delivery failure to the
/// caller (unlike incident notifications, which only ever log failures).
#[derive(Clone, Default)]
struct AlwaysFailNotificationService;

#[async_trait]
impl NotificationService for AlwaysFailNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        Err("simulated delivery failure".into())
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn build_test_app_with_notification_service<S>(notification_service: S) -> axum::Router
where
    S: NotificationService + 'static,
{
    let monitor_repository = InMemoryMonitorRepository::new();

    let state = AppState::new(
        monitor_repository.clone(),
        NoopCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repository),
        notification_service,
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

fn build_test_app() -> axum::Router {
    build_test_app_with_notification_service(NoopNotificationService::new())
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

fn monitor_payload(user_id: Uuid) -> Value {
    json!({
        "user_id": user_id.to_string(),
        "name": "Test Monitor",
        "url": "https://example.com",
        "method": HttpMethod::Get,
        "expected_status": 200,
        "interval_seconds": 60,
        "timeout_seconds": 10,
        "assertions": []
    })
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_monitor(app: &axum::Router, user_id: Uuid) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/monitors")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(monitor_payload(user_id).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;
    body["id"].as_str().unwrap().to_string()
}

async fn create_channel(
    app: &axum::Router,
    user_id: Uuid,
    monitor_id: &str,
    payload: Value,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/monitors/{monitor_id}/notifications"))
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_email_channel_succeeds_and_hides_signing_secret() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let response = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;

    assert_eq!(body["kind"], "email");
    assert_eq!(body["target"], "owner@example.com");
    assert_eq!(body["has_signing_secret"], false);
    // Email channels never carry a signing secret, so the field — present
    // for webhook channels — must be null here.
    assert!(body["signing_secret"].is_null());
}

#[tokio::test]
async fn create_webhook_channel_returns_signing_secret_once_then_hides_it() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let response = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "webhook", "target": "https://example.com/hooks/incoming" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;

    assert_eq!(body["kind"], "webhook");
    assert_eq!(body["has_signing_secret"], true);

    let signing_secret = body["signing_secret"]
        .as_str()
        .expect("webhook channel must return a signing_secret on creation");
    assert!(
        signing_secret.starts_with("whsec_"),
        "expected whsec_ prefix, got {signing_secret}"
    );

    // Now list the channels for this monitor: the secret must NOT come
    // back a second time, anywhere in the response.
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}/notifications"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = read_json(list_response).await;
    let channels = list_body.as_array().unwrap();
    assert_eq!(channels.len(), 1);

    let listed = channels[0].to_string();
    assert!(
        !listed.contains(signing_secret),
        "signing secret leaked back out through the list endpoint: {listed}"
    );
    assert!(
        !listed.contains("\"signing_secret\":"),
        "list response must not even carry a signing_secret field: {listed}"
    );
    assert_eq!(channels[0]["has_signing_secret"], true);
}

#[tokio::test]
async fn delete_channel_removes_it() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let created = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;
    let channel_id = read_json(created).await["id"].as_str().unwrap().to_string();

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/notifications/{channel_id}"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}/notifications"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let list_body = read_json(list_response).await;
    assert_eq!(list_body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_notification_endpoint_reports_success() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let created = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;
    let channel_id = read_json(created).await["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/notifications/{channel_id}/test"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["status"], "sent");
}

#[tokio::test]
async fn test_notification_endpoint_surfaces_delivery_failure() {
    let app = build_test_app_with_notification_service(AlwaysFailNotificationService);
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let created = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;
    let channel_id = read_json(created).await["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/notifications/{channel_id}/test"))
                .header("authorization", auth_header(user_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // A test-send failure IS surfaced to the caller (unlike incident
    // notifications, which only ever get logged).
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rejects_invalid_email_target() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let response = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "email", "target": "not-an-email" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_invalid_webhook_url() {
    let app = build_test_app();
    let user_id = Uuid::new_v4();
    let monitor_id = create_monitor(&app, user_id).await;

    let response = create_channel(
        &app,
        user_id,
        &monitor_id,
        json!({ "kind": "webhook", "target": "not a url" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Ownership / IDOR
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cannot_create_channel_on_another_users_monitor() {
    let app = build_test_app();
    let owner = Uuid::new_v4();
    let attacker = Uuid::new_v4();

    let monitor_id = create_monitor(&app, owner).await;

    // `attacker` guesses/enumerates `owner`'s monitor id and tries to
    // attach a notification channel to it.
    let response = create_channel(
        &app,
        attacker,
        &monitor_id,
        json!({ "kind": "email", "target": "attacker@example.com" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // And the owner's channel list is untouched.
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}/notifications"))
                .header("authorization", auth_header(owner))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let list_body = read_json(list_response).await;
    assert_eq!(list_body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn cannot_list_channels_on_another_users_monitor() {
    let app = build_test_app();
    let owner = Uuid::new_v4();
    let attacker = Uuid::new_v4();

    let monitor_id = create_monitor(&app, owner).await;
    create_channel(
        &app,
        owner,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}/notifications"))
                .header("authorization", auth_header(attacker))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // 404, NOT 200 with an empty array — an empty 200 would confirm to the
    // attacker that this monitor id exists, just isn't theirs.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cannot_delete_another_users_channel() {
    let app = build_test_app();
    let owner = Uuid::new_v4();
    let attacker = Uuid::new_v4();

    let monitor_id = create_monitor(&app, owner).await;
    let created = create_channel(
        &app,
        owner,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;
    let channel_id = read_json(created).await["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/notifications/{channel_id}"))
                .header("authorization", auth_header(attacker))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Channel must still be there for the real owner.
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/monitors/{monitor_id}/notifications"))
                .header("authorization", auth_header(owner))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let list_body = read_json(list_response).await;
    assert_eq!(list_body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn cannot_send_test_notification_for_another_users_channel() {
    let app = build_test_app();
    let owner = Uuid::new_v4();
    let attacker = Uuid::new_v4();

    let monitor_id = create_monitor(&app, owner).await;
    let created = create_channel(
        &app,
        owner,
        &monitor_id,
        json!({ "kind": "email", "target": "owner@example.com" }),
    )
    .await;
    let channel_id = read_json(created).await["id"].as_str().unwrap().to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/notifications/{channel_id}/test"))
                .header("authorization", auth_header(attacker))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
