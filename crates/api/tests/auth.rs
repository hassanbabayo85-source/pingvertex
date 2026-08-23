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
//! Follows the exact test-bootstrap pattern used by
//! `crates/api/tests/monitors.rs` (in-memory repos + `build_test_router`,
//! no live Postgres needed for these tests).

use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    BackupCodeRepository, CheckRepository, DiagnosticReportRepository, IncidentRepository,
    PendingAuthCache, PendingAuthCacheResult, RegistrationTransaction, SecretCipher,
    SecretCipherError, UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{
    Request, StatusCode,
    header::{AUTHORIZATION, SET_COOKIE},
};
use chrono::{DateTime, Duration, Utc};
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
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

/// Same secret used to build the test app's `JsonWebTokenService` below —
/// kept as a constant so the "forged token" tests can sign with the exact
/// same (and, separately, a different) key.
const TEST_JWT_SECRET: &str = "test-secret";

#[derive(Serialize)]
struct ForgedClaims {
    sub: String,
    exp: usize,
}

// --- Minimal Noop stand-ins, copied from crates/api/tests/monitors.rs's
// local structs (each integration test file is its own crate, so private
// items in monitors.rs are not reusable here). ---

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

/// B4: minimal, real (in-process) `PendingAuthCache` — unlike
/// `NoopPendingAuthCache` (which `AuthState::new` defaults to, and which
/// `build_test_app` used exclusively before this worker), this one
/// actually persists values for the lifetime of one `build_test_app()`
/// call, which the login-lockout tests below need (they rely on a
/// failed-attempt counter surviving across several sequential requests
/// against the SAME app instance).
///
/// KNOWN LIMITATION (matches HANDOVER.md "Known Risks"): TTL is accepted
/// but ignored — entries never expire in this test double. None of the
/// tests in this file exercise lockout *expiry*, only the
/// increment/lockout/reset behavior within a single test, so this is
/// sufficient for what's tested; it must not be reused for a future test
/// that needs TTL semantics.
#[derive(Clone, Default)]
struct InMemoryPendingAuthCache {
    store: Arc<tokio::sync::Mutex<HashMap<String, String>>>,
}

#[async_trait]
impl PendingAuthCache for InMemoryPendingAuthCache {
    async fn get(&self, key: &str) -> PendingAuthCacheResult<Option<String>> {
        Ok(self.store.lock().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str, _ttl_seconds: u64) -> PendingAuthCacheResult<()> {
        self.store
            .lock()
            .await
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    async fn delete(&self, key: &str) -> PendingAuthCacheResult<()> {
        self.store.lock().await.remove(key);
        Ok(())
    }
}

/// B4: mirrors `AuthState`'s own private `NoopBackupCodeRepository`
/// (`crates/api/src/auth_state.rs`) exactly — needed here only because
/// `AuthState::with_totp_support` requires a `BackupCodeRepository` impl
/// as one of its three arguments; none of these tests touch backup codes.
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
    async fn find_unused_by_user(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<BackupCode>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    async fn mark_used(
        &self,
        _id: BackupCodeId,
        _used_at: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn delete_all_for_user(
        &self,
        _user_id: UserId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// Bridges `RegisterUserUseCase`'s atomic-transaction port to the
/// in-memory `UserRepository` these tests use for `find_by_email` lookups
/// during login, so a register -> login round trip actually round-trips.
/// Tenant/membership persistence is intentionally skipped: `jwt_auth`'s
/// tenant resolution goes through `InMemoryTenantMembershipRepository`,
/// which auto-vivifies a membership for any user id regardless of what
/// was (or wasn't) written here.
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
    // B4: wires a REAL (in-memory) PendingAuthCache instead of leaving
    // the `NoopPendingAuthCache` default — the login-lockout feature
    // added in this worker reads/writes through this same port
    // (`AuthState::pending_auth_cache`), so without this the lockout
    // tests below would silently never trigger (every `get` would
    // return `None`, every `set`/`delete` would be a no-op).
    .with_totp_support(
        NoopSecretCipher,
        InMemoryPendingAuthCache::default(),
        NoopBackupCodeRepository,
    )
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

fn register_payload(username: &str, email: &str, password: &str) -> Value {
    json!({ "username": username, "email": email, "password": password })
}

fn login_payload(email: &str, password: &str) -> Value {
    json!({ "email": email, "password": password })
}

const STRONG_PASSWORD: &str = "SecurePassword123!";

fn unique_user() -> (String, String) {
    // Username::new enforces a 32-char max; take just the first 12 hex
    // chars of a fresh UUID so "user" + suffix always stays well under
    // that limit while remaining unique per test invocation.
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

/// Extracts just the `access_token` value out of a `Set-Cookie` header,
/// e.g. `"access_token=abc; HttpOnly; ..."` -> `"abc"`.
fn access_token_value(set_cookie: &str) -> String {
    set_cookie
        .strip_prefix("access_token=")
        .expect("cookie should start with access_token=")
        .split(';')
        .next()
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
    assert!(set_cookie.contains("Secure"));
    assert!(set_cookie.contains("SameSite=Strict"));

    let body = read_json(response).await;
    assert_eq!(body["authenticated"], true);
}

#[tokio::test]
async fn register_endpoint_rejects_duplicate_email_with_generic_error() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let first = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    // Different username, SAME email.
    let (other_username, _) = unique_user();
    let second = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&other_username, &email, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(second.status(), StatusCode::BAD_REQUEST);
    let body = read_json(second).await;
    // Generic message: must not reveal that it was specifically the email
    // (as opposed to the username) that collided.
    assert_eq!(body["message"], "registration failed");
}

#[tokio::test]
async fn register_endpoint_rejects_weak_password() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, "short"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// --------------------------------- login ---------------------------------

#[tokio::test]
async fn login_endpoint_succeeds_with_correct_credentials_and_sets_cookie() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let login_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(login_response.status(), StatusCode::OK);
    let set_cookie = set_cookie_header(&login_response);
    assert!(set_cookie.starts_with("access_token="));

    let body = read_json(login_response).await;
    assert_eq!(body["authenticated"], true);
}

#[tokio::test]
async fn login_endpoint_rejects_wrong_password_with_generic_error() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let login_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, "WrongPassword999!"),
    )
    .await;

    assert_eq!(login_response.status(), StatusCode::BAD_REQUEST);
    let body = read_json(login_response).await;
    assert_eq!(body["message"], "invalid email or password");
}

#[tokio::test]
async fn login_endpoint_nonexistent_email_and_wrong_password_for_existing_user_look_identical() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let wrong_password_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, "WrongPassword999!"),
    )
    .await;
    let nonexistent_email_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload("never-registered@example.com", "WrongPassword999!"),
    )
    .await;

    assert_eq!(wrong_password_response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(nonexistent_email_response.status(), StatusCode::BAD_REQUEST);

    let wrong_password_body = read_json(wrong_password_response).await;
    let nonexistent_email_body = read_json(nonexistent_email_response).await;

    assert_eq!(wrong_password_body["message"], "invalid email or password");
    assert_eq!(
        nonexistent_email_body["message"],
        "invalid email or password"
    );
}

// ------------------------- B4: brute-force lockout -------------------------

/// Must match `MAX_FAILED_LOGIN_ATTEMPTS` in
/// `crates/application/src/use_cases/login_user.rs`.
const MAX_FAILED_LOGIN_ATTEMPTS: u32 = 5;

/// Must match `LOGIN_LOCKOUT_MESSAGE` in both `login_user.rs` and
/// `crates/api/src/handlers/login.rs`.
const LOGIN_LOCKOUT_MESSAGE: &str =
    "account temporarily locked due to too many failed login attempts; please try again later";

#[tokio::test]
async fn repeated_failed_logins_lock_the_account_even_against_the_correct_password() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);

    // MAX_FAILED_LOGIN_ATTEMPTS wrong-password attempts should each be an
    // ordinary 400 "invalid email or password".
    for attempt in 0..MAX_FAILED_LOGIN_ATTEMPTS {
        let response = post(
            &app,
            "/api/v1/auth/login",
            login_payload(&email, "WrongPassword999!"),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "attempt {attempt} should be a plain wrong-password rejection, not yet locked"
        );
        let body = read_json(response).await;
        assert_eq!(body["message"], "invalid email or password");
    }

    // The next attempt — even with the CORRECT password — must be
    // rejected with 429, proving the account is now locked rather than
    // credentials still being checked.
    let locked_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(locked_response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = read_json(locked_response).await;
    assert_eq!(body["message"], LOGIN_LOCKOUT_MESSAGE);
}

#[tokio::test]
async fn lockout_applies_identically_to_a_nonexistent_email_no_enumeration_oracle() {
    let app = build_test_app();
    let (_username, email) = unique_user(); // deliberately never registered

    for _ in 0..MAX_FAILED_LOGIN_ATTEMPTS {
        let response = post(
            &app,
            "/api/v1/auth/login",
            login_payload(&email, "WhateverPassword999!"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    let locked_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, "WhateverPassword999!"),
    )
    .await;

    // Same 429 + same message as the "real, registered account" lockout
    // test above — an attacker cannot distinguish "this email doesn't
    // exist" from "this real account is now locked out": both look
    // identical, because the failed-attempt counter is incremented the
    // same way regardless of whether the email belongs to a real user.
    assert_eq!(locked_response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = read_json(locked_response).await;
    assert_eq!(body["message"], LOGIN_LOCKOUT_MESSAGE);
}

// ------------------- B4: IP-based rate limiting (burst) -------------------

#[tokio::test]
async fn burst_requests_to_login_are_rate_limited_by_ip() {
    let user_repository = InMemoryUserRepository::new();
    let auth_state = build_auth_state(user_repository);

    // Uses the dedicated test helper (see its doc comment in router.rs)
    // that mounts /api/v1/auth/* with the SAME GovernorLayer config
    // build_router applies in production (5 req/60s/IP, burst of 5) —
    // build_test_router omits this layer entirely because bare
    // `oneshot()` calls don't supply `ConnectInfo<SocketAddr>` the way a
    // real listener does, so we supply it manually below instead.
    let app = api::router::build_auth_only_router_with_rate_limit(auth_state);
    let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
    let (_username, email) = unique_user();

    let mut statuses = Vec::new();
    // burst_size 5: the first 5 requests in immediate succession should
    // be let through (as ordinary 400s — this email/password combo is
    // wrong on purpose, since this test only cares about IP-based rate
    // limiting, not credential correctness), and the 6th should be
    // rejected with 429 before it ever reaches the login handler.
    for _ in 0..6 {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(
                login_payload(&email, "WrongPassword999!").to_string(),
            ))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(addr));

        let status = app.clone().oneshot(request).await.unwrap().status();
        statuses.push(status);
    }

    assert_eq!(
        statuses.last().copied(),
        Some(StatusCode::TOO_MANY_REQUESTS),
        "expected the 6th rapid request from the same IP to be rate-limited, got: {statuses:?}"
    );
}

// --------------------------------- logout ---------------------------------

#[tokio::test]
async fn logout_endpoint_clears_session_cookie() {
    let app = build_test_app();

    let response = post(&app, "/api/v1/auth/logout", json!({})).await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = set_cookie_header(&response);
    assert!(set_cookie.starts_with("access_token=;"));
    assert!(set_cookie.contains("Max-Age=0"));

    let body = read_json(response).await;
    assert_eq!(body["authenticated"], false);
}

// ----------------------- session / protected-route guard -----------------------

#[tokio::test]
async fn protected_route_rejects_missing_token() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_rejects_malformed_token() {
    let app = build_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .header(AUTHORIZATION, "Bearer not-a-real-jwt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_rejects_expired_token() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let set_cookie = set_cookie_header(&register_response);
    let valid_token = access_token_value(&set_cookie);

    // Decode just enough to reuse the same `sub`, then re-sign with an
    // already-expired `exp`, using the exact secret this test app's
    // JsonWebTokenService was built with.
    let claims = jsonwebtoken::decode::<serde_json::Value>(
        &valid_token,
        &jsonwebtoken::DecodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
        &{
            let mut v = jsonwebtoken::Validation::default();
            v.validate_exp = false; // the token isn't expired yet at this point
            v
        },
    )
    .unwrap()
    .claims;
    let sub = claims["sub"].as_str().unwrap().to_string();

    let expired_claims = ForgedClaims {
        sub,
        exp: (Utc::now() - Duration::hours(1)).timestamp() as usize,
    };
    let expired_token = encode(
        &Header::default(),
        &expired_claims,
        &EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
    )
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .header(AUTHORIZATION, format!("Bearer {expired_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_accepts_valid_session_cookie_from_login() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let login_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);
    let set_cookie = set_cookie_header(&login_response);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .header("cookie", set_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn protected_route_accepts_valid_bearer_header_fallback() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let set_cookie = set_cookie_header(&register_response);
    let token = access_token_value(&set_cookie);

    // No cookie header at all here — only the Authorization header, to
    // exercise jwt_auth's cookie-first-then-header-fallback path.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

fn change_password_payload(current_password: &str, new_password: &str) -> Value {
    json!({ "current_password": current_password, "new_password": new_password })
}

fn cookie_header_value(set_cookie: &str) -> String {
    format!("access_token={}", access_token_value(set_cookie))
}

async fn post_authed(
    app: &axum::Router,
    uri: &str,
    body: Value,
    set_cookie: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", cookie_header_value(set_cookie))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn session_is_valid(app: &axum::Router, set_cookie: &str) -> bool {
    app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/monitors")
                .header("cookie", cookie_header_value(set_cookie))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
        == StatusCode::OK
}

const NEW_STRONG_PASSWORD: &str = "NewSecurePassword456!";

#[tokio::test]
async fn change_password_happy_path_and_old_session_rejected() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let old_set_cookie = set_cookie_header(&register_response);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    assert!(
        session_is_valid(&app, &old_set_cookie).await,
        "session should be valid before password change"
    );

    let change_response = post_authed(
        &app,
        "/api/v1/auth/password/change",
        change_password_payload(STRONG_PASSWORD, NEW_STRONG_PASSWORD),
        &old_set_cookie,
    )
    .await;

    assert_eq!(change_response.status(), StatusCode::NO_CONTENT);

    assert!(
        !session_is_valid(&app, &old_set_cookie).await,
        "session should be invalid after password change"
    );

    let login_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, NEW_STRONG_PASSWORD),
    )
    .await;

    assert_eq!(login_response.status(), StatusCode::OK);
    let new_set_cookie = set_cookie_header(&login_response);
    assert!(
        session_is_valid(&app, &new_set_cookie).await,
        "new session should be valid"
    );
}

#[tokio::test]
async fn change_password_rejects_wrong_current_password() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let set_cookie = set_cookie_header(&register_response);

    let change_response = post_authed(
        &app,
        "/api/v1/auth/password/change",
        change_password_payload("WrongCurrentPassword999!", NEW_STRONG_PASSWORD),
        &set_cookie,
    )
    .await;

    assert_eq!(change_response.status(), StatusCode::UNAUTHORIZED);
    let body = read_json(change_response).await;
    assert_eq!(body["message"], "invalid current password");

    let login_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn change_password_rejects_weak_new_password() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let set_cookie = set_cookie_header(&register_response);

    let change_response = post_authed(
        &app,
        "/api/v1/auth/password/change",
        change_password_payload(STRONG_PASSWORD, "short"),
        &set_cookie,
    )
    .await;

    assert_eq!(change_response.status(), StatusCode::BAD_REQUEST);
    let body = read_json(change_response).await;
    assert!(body["message"].as_str().unwrap().contains("password"));
}

#[tokio::test]
async fn change_password_invalidates_all_other_sessions() {
    let app = build_test_app();
    let (username, email) = unique_user();

    let register_response = post(
        &app,
        "/api/v1/auth/register",
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register_response.status(), StatusCode::CREATED);
    let session_a = set_cookie_header(&register_response);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let login_response = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);
    let session_b = set_cookie_header(&login_response);

    assert!(session_is_valid(&app, &session_a).await);
    assert!(session_is_valid(&app, &session_b).await);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let change_response = post_authed(
        &app,
        "/api/v1/auth/password/change",
        change_password_payload(STRONG_PASSWORD, NEW_STRONG_PASSWORD),
        &session_a,
    )
    .await;
    assert_eq!(change_response.status(), StatusCode::NO_CONTENT);

    assert!(
        !session_is_valid(&app, &session_a).await,
        "session_a should be invalid"
    );
    assert!(
        !session_is_valid(&app, &session_b).await,
        "session_b should be invalid"
    );

    let fresh_login = post(
        &app,
        "/api/v1/auth/login",
        login_payload(&email, NEW_STRONG_PASSWORD),
    )
    .await;
    assert_eq!(fresh_login.status(), StatusCode::OK);
    let fresh_session = set_cookie_header(&fresh_login);
    assert!(session_is_valid(&app, &fresh_session).await);
}
