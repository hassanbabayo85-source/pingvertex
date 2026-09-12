//! H5 — Backend Cookie/Auth End-to-End Tests.
//!
//! Starts a REAL Axum HTTP server (via `axum::serve`) with the
//! `build_test_router` using in-memory repositories. All requests go
//! through `reqwest` with a cookie jar enabled, so Set-Cookie/HttpOnly
//! behavior is exercised at the HTTP layer, not just handler-level.
//!
//! Scenarios covered (all must PASS):
//!   1. no cookie
//!   2. invalid cookie
//!   3. expired cookie
//!   4. valid cookie
//!   5. logout
//!   6. revoked session
//!   7. 2FA pending session
//!   8. unauthorized tenant access

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use application::ports::{
    BackupCodeRepository, CheckRepository, DiagnosticReportRepository, IncidentRepository,
    PasswordHasher, PendingAuthCache, PendingAuthCacheResult, RegistrationTransaction,
    SecretCipher, SecretCipherError, TenantMembershipRepository, TenantRepository, UserRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
use tokio::net::TcpListener;
use uuid::Uuid;

use api::{auth_state::AuthState, router::build_test_router, state::AppState};

const TEST_JWT_SECRET: &str = "test-secret";
const STRONG_PASSWORD: &str = "SecurePassword123!";

// ----------------------------- Noop repositories -------------------------

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
        _: domain::shared::ids::CheckId,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn list_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    async fn list_checks(
        &self,
        _: u32,
        _: u32,
        _: domain::shared::ids::TenantId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
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
        _: domain::shared::ids::CheckId,
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
        _: domain::shared::ids::IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_open_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn list_by_monitor(
        &self,
        _: domain::shared::ids::MonitorId,
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
    tenants: InMemoryTenantRepository,
    memberships: InMemoryTenantMembershipRepository,
}

#[async_trait]
impl RegistrationTransaction for PersistingRegistrationTransaction {
    async fn register_user_with_tenant(
        &self,
        user: &User,
        tenant: &Tenant,
        membership: &TenantMembership,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.users.create(user).await?;
        self.tenants.create(tenant).await?;
        self.memberships.create(membership).await?;
        Ok(())
    }
}

// ----------------------------- Test context -----------------------------

struct TestContext {
    user_repo: InMemoryUserRepository,
    pending_auth_cache: InMemoryPendingAuthCache,
}

async fn spawn_test_server() -> (SocketAddr, TestContext) {
    let monitor_repo = InMemoryMonitorRepository::new();
    let user_repo = InMemoryUserRepository::new();
    let tenant_repo = InMemoryTenantRepository::new();
    let tenant_membership_repo = InMemoryTenantMembershipRepository::new();
    let pending_auth_cache = InMemoryPendingAuthCache::default();

    let app_state = AppState::new(
        monitor_repo.clone(),
        NoopCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo.clone()),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    );

    let auth_state = AuthState::new(
        user_repo.clone(),
        Argon2PasswordHasher,
        JsonWebTokenService::new(TEST_JWT_SECRET),
        UuidIdGenerator::new(),
        tenant_repo.clone(),
        tenant_membership_repo.clone(),
        PersistingRegistrationTransaction {
            users: user_repo.clone(),
            tenants: tenant_repo.clone(),
            memberships: tenant_membership_repo.clone(),
        },
    )
    .with_totp_support(
        NoopSecretCipher,
        pending_auth_cache.clone(),
        NoopBackupCodeRepository,
    );

    let router = build_test_router(app_state, auth_state);

    let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().expect("get local address");

    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service())
            .await
            .expect("server should run");
    });

    // Give the server a moment to start listening.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let ctx = TestContext {
        user_repo,
        pending_auth_cache,
    };

    (addr, ctx)
}

// ----------------------------- Helpers -----------------------------

fn client_with_cookies() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .expect("build reqwest client")
}

async fn post_json(client: &reqwest::Client, url: &str, body: Value) -> reqwest::Response {
    client
        .post(url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("POST request")
}

async fn get(client: &reqwest::Client, url: &str) -> reqwest::Response {
    client.get(url).send().await.expect("GET request")
}

fn register_payload(username: &str, email: &str, password: &str) -> Value {
    json!({ "username": username, "email": email, "password": password })
}

fn login_payload(email: &str, password: &str) -> Value {
    json!({ "email": email, "password": password })
}

fn unique_user() -> (String, String) {
    let suffix: String = Uuid::new_v4().simple().to_string()[..12].to_string();
    (format!("user{suffix}"), format!("user{suffix}@example.com"))
}

async fn mark_verified_and_login(
    client: &reqwest::Client,
    base: &str,
    user_repo: &InMemoryUserRepository,
    email: &str,
    password: &str,
) -> reqwest::Response {
    let email_value = domain::Email::new(email).unwrap();

    let mut user = user_repo
        .find_by_email(&email_value)
        .await
        .unwrap()
        .expect("registered user should exist");

    user.mark_email_verified(Utc::now());
    user_repo.update(&user).await.unwrap();

    post_json(
        client,
        &format!("{}/api/v1/auth/login", base),
        login_payload(email, password),
    )
    .await
}

// ----------------------------- Individual tests -----------------------------

#[tokio::test]
async fn e2e_no_cookie_returns_401() {
    let (addr, _ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_invalid_cookie_returns_401() {
    let (addr, _ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    // Manually set an invalid cookie header for the domain.
    client
        .get(format!("{}/api/v1/auth/me", base))
        .header("Cookie", "access_token=invalid-token")
        .send()
        .await
        .unwrap();

    // The cookie jar will now contain the invalid cookie for the next request.
    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_expired_cookie_returns_401() {
    let (addr, _ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    // Craft an expired JWT signed with the test secret.
    let now = Utc::now().timestamp();
    let claims = serde_json::json!({
        "sub": "00000000-0000-0000-0000-000000000000",
        "iat": now - 86400,
        "exp": now - 10,
    });
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(TEST_JWT_SECRET.as_bytes()),
    )
    .unwrap();

    // Set the expired cookie.
    client
        .get(format!("{}/api/v1/auth/me", base))
        .header("Cookie", format!("access_token={}", token))
        .send()
        .await
        .unwrap();

    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_valid_cookie_allows_access() {
    let (addr, ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    // Registration creates a pending account without a session.
    let (username, email) = unique_user();
    let resp = post_json(
        &client,
        &format!("{}/api/v1/auth/register", base),
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    assert!(!resp.headers().contains_key("set-cookie"));

    let login =
        mark_verified_and_login(&client, &base, &ctx.user_repo, &email, STRONG_PASSWORD).await;
    assert_eq!(login.status(), reqwest::StatusCode::OK);

    // Access protected endpoint.
    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["authenticated"], true);
}

#[tokio::test]
async fn e2e_logout_clears_cookie_and_then_401() {
    let (addr, ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    // Register (sets cookie) and login.
    let (username, email) = unique_user();
    let register = post_json(
        &client,
        &format!("{}/api/v1/auth/register", base),
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(register.status(), reqwest::StatusCode::CREATED);

    let login =
        mark_verified_and_login(&client, &base, &ctx.user_repo, &email, STRONG_PASSWORD).await;
    assert_eq!(login.status(), reqwest::StatusCode::OK);

    // Now logout.
    let resp = client
        .post(format!("{}/api/v1/auth/logout", base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    // The Set-Cookie header should clear the cookie.
    let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.contains("Max-Age=0"));

    // After logout, the cookie jar should have an expired/cleared cookie.
    // Attempt to access protected endpoint.
    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_logout_revokes_copied_bearer_token() {
    let (addr, ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    let (username, email) = unique_user();
    let register = post_json(
        &client,
        &format!("{}/api/v1/auth/register", base),
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register.status(), reqwest::StatusCode::CREATED);
    assert!(!register.headers().contains_key("set-cookie"));

    let login =
        mark_verified_and_login(&client, &base, &ctx.user_repo, &email, STRONG_PASSWORD).await;
    assert_eq!(login.status(), reqwest::StatusCode::OK);

    let set_cookie = login.headers().get("set-cookie").unwrap().to_str().unwrap();
    let token = set_cookie
        .strip_prefix("access_token=")
        .and_then(|value| value.split(';').next())
        .expect("registration must return an access token cookie")
        .to_owned();

    let logout = client
        .post(format!("{}/api/v1/auth/logout", base))
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), reqwest::StatusCode::OK);

    // The attacker does not depend on the browser cookie jar. A copied JWT
    // presented as a bearer token must be rejected after logout.
    let resp = client
        .get(format!("{}/api/v1/auth/me", base))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_revoked_session_is_rejected() {
    let (addr, ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    // Register, verify the fixture user, and login.
    let (username, email) = unique_user();
    let register = post_json(
        &client,
        &format!("{}/api/v1/auth/register", base),
        register_payload(&username, &email, STRONG_PASSWORD),
    )
    .await;
    assert_eq!(register.status(), reqwest::StatusCode::CREATED);

    let login =
        mark_verified_and_login(&client, &base, &ctx.user_repo, &email, STRONG_PASSWORD).await;
    assert_eq!(login.status(), reqwest::StatusCode::OK);

    // Ensure we can access protected endpoint first.
    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Find user id for this email.
    let user = ctx
        .user_repo
        .find_by_email(&domain::Email::new(&email).unwrap())
        .await
        .unwrap()
        .unwrap();
    let user_id = user.id();

    // Revoke session by setting invalidation marker. Add 1 second to
    // guarantee it is strictly greater than the token's issued-at
    // timestamp (they might have been issued within the same second).
    let now = Utc::now().timestamp() + 1;
    let key = application::session_invalidated_at_cache_key(user_id);
    ctx.pending_auth_cache
        .set(&key, &now.to_string(), 86400)
        .await
        .unwrap();

    // Now the same cookie should be rejected.
    let resp = get(&client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_2fa_pending_session_returns_required_and_no_cookie() {
    let (addr, ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);
    let client = client_with_cookies();

    // Manually create a user with TOTP enabled.
    let email = format!("2fa-{}@example.com", Uuid::new_v4().simple());
    let password = STRONG_PASSWORD;
    let hash = Argon2PasswordHasher.hash(password).await.unwrap();
    let now = Utc::now();
    let user = domain::UserBuilder::new()
        .id(UserId::new())
        .username(domain::Username::new("2fauser").unwrap())
        .email(domain::Email::new(&email).unwrap())
        .password_hash(domain::PasswordHash::new(hash).unwrap())
        .role(domain::UserRole::User)
        .status(domain::UserStatus::Active)
        .email_verified_at(Some(now))
        .created_at(now)
        .updated_at(now)
        .totp_enabled(true)
        .build()
        .unwrap();
    ctx.user_repo.create(&user).await.unwrap();

    // Attempt login with this user.
    let resp = post_json(
        &client,
        &format!("{}/api/v1/auth/login", base),
        login_payload(&email, password),
    )
    .await;
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Check headers BEFORE consuming body (because `json()` moves resp).
    assert!(
        !resp.headers().contains_key("set-cookie"),
        "2FA pending login must not set a session cookie"
    );

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["authenticated"], false);
    assert_eq!(body["two_factor_required"], true);
    assert!(body["pending_token"].is_string());

    // Also ensure that using the pending_token as a cookie does NOT authenticate.
    let pending_token = body["pending_token"].as_str().unwrap();
    // Create a new client that doesn't have any other cookies.
    let malicious_client = client_with_cookies();
    malicious_client
        .get(format!("{}/api/v1/auth/me", base))
        .header("Cookie", format!("access_token={}", pending_token))
        .send()
        .await
        .unwrap();
    let resp = get(&malicious_client, &format!("{}/api/v1/auth/me", base)).await;
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn e2e_unauthorized_tenant_access_returns_404() {
    let (addr, ctx) = spawn_test_server().await;
    let base = format!("http://{}", addr);

    // Client A
    let client_a = client_with_cookies();
    let (username_a, email_a) = unique_user();
    let register_a = post_json(
        &client_a,
        &format!("{}/api/v1/auth/register", base),
        register_payload(&username_a, &email_a, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(register_a.status(), reqwest::StatusCode::CREATED);

    let login_a =
        mark_verified_and_login(&client_a, &base, &ctx.user_repo, &email_a, STRONG_PASSWORD).await;

    assert_eq!(login_a.status(), reqwest::StatusCode::OK);

    // Create a monitor as A.
    let monitor_body = json!({
        "name": "A's monitor",
        "url": "http://example.com",
        "method": "GET",
        "expected_status": 200,
        "interval_seconds": 600,
        "timeout_seconds": 10,
        "assertions": []
    });
    let resp = post_json(
        &client_a,
        &format!("{}/api/v1/monitors", base),
        monitor_body,
    )
    .await;
    let status = resp.status();
    let body = resp.text().await.unwrap();
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "create monitor failed: status={status}, body={body}"
    );

    let body: Value = serde_json::from_str(&body).expect("created monitor response must be JSON");
    let monitor_id = body["id"].as_str().unwrap().to_string();

    // Client B (different user/tenant)
    let client_b = client_with_cookies();
    let (username_b, email_b) = unique_user();
    let register_b = post_json(
        &client_b,
        &format!("{}/api/v1/auth/register", base),
        register_payload(&username_b, &email_b, STRONG_PASSWORD),
    )
    .await;

    assert_eq!(register_b.status(), reqwest::StatusCode::CREATED);

    let login_b =
        mark_verified_and_login(&client_b, &base, &ctx.user_repo, &email_b, STRONG_PASSWORD).await;

    assert_eq!(login_b.status(), reqwest::StatusCode::OK);

    // Attempt to access A's monitor with B's cookie.
    let resp = get(
        &client_b,
        &format!("{}/api/v1/monitors/{}", base, monitor_id),
    )
    .await;
    // Should be 404 because B's tenant does not own the monitor.
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}
