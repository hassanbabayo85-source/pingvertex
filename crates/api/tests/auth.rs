//! B3 — HTTP-layer integration tests for register / login / logout /
//! session validation.
//!
//! Scope note: register/login/logout and the `jwt_auth` session-validation
//! middleware were already implemented (including argon2 hashing, JWT
//! expiration, generic error messages, and TOTP support) before B3 ran.
//! B3 added the HTTP-level integration test coverage; it did not change
//! any production auth code. See HANDOVER.md "Deviations from prompt"
//! for the one small infra addition (`InMemoryUserRepository`) that
//! required.
//!
//! B4 — added: per-account login brute-force lockout tests, an
//! IP-based rate-limit burst test, and (via `login_user.rs` /
//! `login.rs`) a fix so login error messages never distinguish
//! "wrong password" / "nonexistent email" / "account suspended" / "cache
//! failure" from one another — see login_user.rs's own doc comments for
//! the audit findings. `build_test_app()` below now wires a real
//! (in-memory) `PendingAuthCache` instead of the previous `NoopPendingAuthCache`
//! default, since the lockout tests need failed-attempt counts to
//! actually persist across requests within one test.
//!
//! H2 — added: forgot-password / resend-verification rate limiting tests
//! using `InMemoryRateLimiter` (simulating Redis-backed behavior).
//!
//! H3 — updated: `InMemoryPendingAuthCache` now implements the atomic
//! `getdel` method and `NoopBackupCodeRepository` returns `bool` from
//! `mark_used`.

use api::{
    auth_state::{AuthState, InMemoryRateLimiter},
    router::build_test_router,
    state::AppState,
};
use application::ports::{
    BackupCodeRepository, CheckRepository, DiagnosticReportRepository, IncidentRepository,
    PendingAuthCache, PendingAuthCacheResult, RateLimiter, RateLimiterResult,
    RegistrationTransaction, SecretCipher, SecretCipherError, UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::SET_COOKIE};
use chrono::{DateTime, Utc};
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId};
use domain::{
    BackupCode, BackupCodeId, Check, DiagnosticReport, Incident, Tenant, TenantMembership, User,
    UserId,
};
use infra::{
    Argon2PasswordHasher, InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, InMemoryTenantMembershipRepository, InMemoryTenantRepository,
    InMemoryUserRepository, JsonWebTokenService, NoopNotificationService, UuidIdGenerator,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_JWT_SECRET: &str = "test-secret";

// --- Minimal Noop stand-ins, copied from crates/api/tests/monitors.rs's
// local structs ---

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

#[derive(Clone, Default)]
struct InMemoryPendingAuthCache {
    store: Arc<tokio::sync::Mutex<HashMap<String, (String, i64)>>>,
}

#[async_trait]
impl PendingAuthCache for InMemoryPendingAuthCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        let store = self.store.lock().await;
        Ok(store.get(key).map(|(value, _)| value.clone()))
    }

    async fn getdel(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        let mut store = self.store.lock().await;
        Ok(store.remove(key).map(|(value, _)| value))
    }

    async fn set(&self, key: &str, value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        let mut store = self.store.lock().await;
        store.insert(key.to_string(), (value.to_string(), Utc::now().timestamp()));
        Ok(())
    }

    async fn delete(&self, key: &str) -> PendingAuthCacheResult<()> {
        let mut store = self.store.lock().await;
        store.remove(key);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopBackupCodeRepository;

#[async_trait]
impl BackupCodeRepository for NoopBackupCodeRepository {
    async fn create_many(
        &self,
        _codes: &[BackupCode],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_all_by_user(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<BackupCode>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    async fn mark_used(
        &self,
        _id: BackupCodeId,
        _used_at: DateTime<Utc>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(true)
    }
    async fn delete_all_for_user(
        &self,
        _user_id: UserId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[derive(Clone)]
struct PersistingRegistrationTransaction {
    users: InMemoryUserRepository,
}

#[async_trait]
impl RegistrationTransaction for PersistingRegistrationTransaction {
    async fn register_user_with_tenant(
        &self,
        user: &User,
        _tenant: &Tenant,
        _membership: &TenantMembership,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.users.create(user).await
    }
}

#[derive(Clone, Default)]
struct FailingRateLimiter;

#[async_trait]
impl RateLimiter for FailingRateLimiter {
    async fn check_and_record(
        &self,
        _namespace: &str,
        _key: &str,
        _max_requests: u32,
        _window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        Err("simulated redis outage".into())
    }
}

fn build_auth_state(user_repository: InMemoryUserRepository) -> AuthState {
    AuthState::new(
        user_repository.clone(),
        Argon2PasswordHasher,
        JsonWebTokenService::new(TEST_JWT_SECRET),
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        InMemoryTenantMembershipRepository::new(),
        PersistingRegistrationTransaction {
            users: user_repository,
        },
    )
    .with_totp_support(
        NoopSecretCipher,
        InMemoryPendingAuthCache::default(),
        NoopBackupCodeRepository,
    )
}

fn build_auth_state_with_rate_limiter<R>(
    user_repository: InMemoryUserRepository,
    rate_limiter: R,
) -> AuthState
where
    R: RateLimiter + 'static,
{
    build_auth_state(user_repository).with_rate_limiter(rate_limiter)
}

fn build_test_app() -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();
    let user_repository = InMemoryUserRepository::new();

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

    let auth_state = build_auth_state(user_repository);

    build_test_router(app_state, auth_state)
}

fn build_test_app_with_rate_limiter<R>(rate_limiter: R) -> axum::Router
where
    R: RateLimiter + 'static,
{
    let monitor_repository = InMemoryMonitorRepository::new();
    let user_repository = InMemoryUserRepository::new();

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

    let auth_state = build_auth_state_with_rate_limiter(user_repository, rate_limiter);

    build_test_router(app_state, auth_state)
}

fn register_payload(username: &str, email: &str, password: &str) -> Value {
    json!({ "username": username, "email": email, "password": password })
}

fn forgot_password_payload(email: &str) -> Value {
    json!({ "email": email })
}

fn resend_verification_payload(email: &str) -> Value {
    json!({ "email": email })
}

fn login_payload(email: &str, password: &str) -> Value {
    json!({ "email": email, "password": password })
}

const STRONG_PASSWORD: &str = "SecurePassword123!";

fn unique_user() -> (String, String) {
    let suffix: String = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(12)
        .collect();
    (format!("user{suffix}"), format!("user{suffix}@example.com"))
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn set_cookie_header(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(SET_COOKIE)
        .expect("expected a Set-Cookie header")
        .to_str()
        .unwrap()
        .to_string()
}

async fn post(app: &axum::Router, uri: &str, body: Value) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

// ------------------------------- register -------------------------------

#[tokio::test]
async fn register_endpoint_creates_user_and_sets_session_cookie() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let set_cookie = set_cookie_header(&response);
    assert!(set_cookie.starts_with("access_token="));
    assert!(set_cookie.contains("HttpOnly"));
}

#[tokio::test]
async fn register_endpoint_rejects_duplicate_email() {
    let app = build_test_app();
    let (username1, email) = unique_user();
    let (username2, _) = unique_user();

    let resp1 = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username1, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::CREATED);

    let resp2 = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username2, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
}

// ------------------------------- login -------------------------------

#[tokio::test]
async fn login_endpoint_sets_session_cookie_on_success() {
    let app = build_test_app();
    let (username, email) = unique_user();
    let password = STRONG_PASSWORD;

    post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, password),
    )
    .await;

    let resp = post(&app, "/api/v1/auth/login", login_payload(&email, password)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = set_cookie_header(&resp);
    assert!(set_cookie.starts_with("access_token="));
}

#[tokio::test]
async fn login_endpoint_rejects_wrong_password() {
    let app = build_test_app();
    let (username, email) = unique_user();
    let password = STRONG_PASSWORD;

    post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, password),
    )
    .await;

    let resp = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, "WrongPassword123!"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = read_json(resp).await;
    assert_eq!(body["message"], "invalid email or password");
}

#[tokio::test]
async fn login_endpoint_rejects_nonexistent_email() {
    let app = build_test_app();
    let resp = post(
        &app,
        "/api/v1/auth/login",
        login_payload("nonexistent@example.com", "Whatever123!"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = read_json(resp).await;
    assert_eq!(body["message"], "invalid email or password");
}

#[tokio::test]
async fn login_endpoint_locks_account_after_repeated_failures() {
    let app = build_test_app();
    let (username, email) = unique_user();
    let password = STRONG_PASSWORD;
    let wrong_password = "NotTheRightOne123!";

    post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, password),
    )
    .await;

    for _ in 0..5 {
        let resp = post(
            &app,
            "/api/v1/auth/login",
            login_payload(&email, wrong_password),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    let resp = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, wrong_password),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ------------------- H2: Forgot-password rate limiting -------------------

#[tokio::test]
async fn forgot_password_first_three_requests_allowed_then_rejected() {
    let limiter = InMemoryRateLimiter::new();
    let app = build_test_app_with_rate_limiter(limiter);
    let email = "forgot-ratelimit@example.com";

    for _ in 0..3 {
        let resp = post(
            &app,
            "/api/v1/auth/forgot-password",
            forgot_password_payload(email),
        )
        .await;
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let resp = post(
        &app,
        "/api/v1/auth/forgot-password",
        forgot_password_payload(email),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn forgot_password_rate_limiter_normalizes_email() {
    let limiter = InMemoryRateLimiter::new();
    let app = build_test_app_with_rate_limiter(limiter);

    let resp1 = post(
        &app,
        "/api/v1/auth/forgot-password",
        forgot_password_payload("  User@Example.COM "),
    )
    .await;
    assert_ne!(resp1.status(), StatusCode::TOO_MANY_REQUESTS);

    for _ in 0..2 {
        let resp = post(
            &app,
            "/api/v1/auth/forgot-password",
            forgot_password_payload("user@example.com"),
        )
        .await;
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let resp = post(
        &app,
        "/api/v1/auth/forgot-password",
        forgot_password_payload("user@example.com"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn forgot_password_fails_closed_when_rate_limiter_unavailable() {
    let app = build_test_app_with_rate_limiter(FailingRateLimiter);

    let resp = post(
        &app,
        "/api/v1/auth/forgot-password",
        forgot_password_payload("failclosed@example.com"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ------------------- H2: Resend-verification rate limiting -------------------

#[tokio::test]
async fn resend_verification_first_three_requests_allowed_then_rejected() {
    let limiter = InMemoryRateLimiter::new();
    let app = build_test_app_with_rate_limiter(limiter);
    let email = "resend-ratelimit@example.com";

    for _ in 0..3 {
        let resp = post(
            &app,
            "/api/v1/auth/resend-verification",
            resend_verification_payload(email),
        )
        .await;
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    let resp = post(
        &app,
        "/api/v1/auth/resend-verification",
        resend_verification_payload(email),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn resend_verification_fails_closed_when_rate_limiter_unavailable() {
    let app = build_test_app_with_rate_limiter(FailingRateLimiter);

    let resp = post(
        &app,
        "/api/v1/auth/resend-verification",
        resend_verification_payload("failclosed@example.com"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
