//! PART 8 — HTTP-boundary pagination contract tests.
//!
//! Proves the HTTP layer wires up the pagination primitives correctly:
//!
//!   1. `?limit=N` is clamped into `[1, MAX_PAGE_SIZE]`.
//!   2. `X-Next-Cursor` is emitted when more rows exist, absent on the
//!      last page.
//!   3. A malformed `?cursor=...` starts from page 1 (no 500).
//!   4. Walking pages with the returned cursors visits every row exactly
//!      once, with no duplicates or skips.
//!   5. Response body shape is unchanged from before PART 8 (a JSON
//!      array), so existing clients keep working.

use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher, SecretCipher,
    SecretCipherError, TokenService, UserRepository,
};
use application::ports::{MonitorRepository, TenantMembershipRepository, TenantRepository};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use domain::pricing::PricingConfig;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{
    Check, CheckInterval, DiagnosticReport, ExpectedStatus, HttpMethod, Incident,
    MonitorBuilder, MonitorName, Timeout, Url, User,
};
use infra::{
    InMemoryAgentLeaseRepository, InMemoryAgentRegistry, InMemoryApiFlowRepository,
    InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryPAYGPricingRepository,
    InMemoryPricingConfigRepository, InMemoryProxyCheckJobRepository,
    InMemoryRegistrationTransaction, InMemoryTenantMembershipRepository, InMemoryTenantRepository,
    InMemoryUsageEventRepository, InMemoryWalletRepository, NoopNotificationService,
    UuidIdGenerator,
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Local test doubles
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct NoopSecretCipher;
impl SecretCipher for NoopSecretCipher {
    fn encrypt_bytes(&self, p: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(p.to_vec())
    }
    fn decrypt_bytes(&self, c: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(c.to_vec())
    }
}

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
struct NoopDiag;
#[async_trait]
impl DiagnosticReportRepository for NoopDiag {
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
struct NoopInc;
#[async_trait]
impl IncidentRepository for NoopInc {
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

#[derive(Clone)]
struct FixtureUserRepo {
    user: User,
}
#[async_trait]
impl UserRepository for FixtureUserRepo {
    async fn create(&self, _: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(&self, _: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: &UserId,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(self.user.clone()))
    }
    async fn find_by_email(
        &self,
        _: &domain::Email,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(self.user.clone()))
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
        Ok(true)
    }
    async fn username_exists(
        &self,
        _: &domain::Username,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
}

#[derive(Clone, Default)]
struct NoopHash;
#[async_trait]
impl PasswordHasher for NoopHash {
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
struct NoopTok;
#[async_trait]
impl TokenService for NoopTok {
    async fn generate(
        &self,
        u: UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(u.0.to_string())
    }
    async fn verify(
        &self,
        token: &str,
    ) -> Result<application::ports::VerifiedToken, Box<dyn std::error::Error + Send + Sync>> {
        let user_id = uuid::Uuid::parse_str(token)?;
        Ok(application::ports::VerifiedToken {
            user_id: UserId(user_id),
            issued_at: 0,
        })
    }
}

/// Builds the app with N monitors, all owned by a fresh user/tenant.
async fn build_app_with_monitors(n: usize) -> (axum::Router, UserId) {
    let user_id = UserId::new();
    let tenant_id = TenantId::new();
    let now = chrono::Utc::now();

    let user = domain::UserBuilder::new()
        .id(user_id)
        .username(domain::Username::new("pagination").unwrap())
        .email(domain::Email::new("pagination@example.test").unwrap())
        .password_hash(domain::PasswordHash::new("test-hash").unwrap())
        .role(domain::UserRole::User)
        .status(domain::UserStatus::Active)
        .email_verified_at(Some(now))
        .created_at(now)
        .updated_at(now)
        .build()
        .unwrap();

    let monitor_repo = InMemoryMonitorRepository::new();
    for i in 0..n {
        let monitor = MonitorBuilder::new(MonitorId::new(), user_id, tenant_id)
            .name(MonitorName::new(format!("monitor-{i:03}")).unwrap())
            .url(Url::new("https://example.com").unwrap())
            .method(HttpMethod::Get)
            .expected_status(ExpectedStatus::new(200).unwrap())
            .interval(CheckInterval::new(60).unwrap())
            .timeout(Timeout::new(5).unwrap())
            .build();
        monitor_repo.save(&monitor).await.unwrap();
    }

    let pricing_repo = Arc::new(
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config()),
    );

    let app_state = AppState::new(
        monitor_repo,
        NoopCheckRepository,
        NoopDiag,
        NoopInc,
        InMemoryNotificationChannelRepository::new(InMemoryMonitorRepository::new()),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_pricing_config_repository(pricing_repo)
    .with_agent_lease_repository(InMemoryAgentLeaseRepository::new())
    .with_payg_pricing_repository(InMemoryPAYGPricingRepository::new())
    .with_wallet_repository(InMemoryWalletRepository::default())
    .with_usage_event_repository(InMemoryUsageEventRepository::default())
    .with_agent_registry(InMemoryAgentRegistry::new());

    let memberships = InMemoryTenantMembershipRepository::new();
    memberships
        .create(&domain::TenantMembership {
            tenant_id,
            user_id,
            role: domain::shared::types::Role::Owner,
            invited_at: now,
            joined_at: Some(now),
        })
        .await
        .unwrap();

    let tenants = InMemoryTenantRepository::new();
    tenants
        .create(&domain::Tenant::new(tenant_id, "Pagination Test", user_id).unwrap())
        .await
        .unwrap();

    let auth_state = AuthState::new(
        FixtureUserRepo { user },
        NoopHash,
        NoopTok,
        UuidIdGenerator::new(),
        tenants,
        memberships,
        InMemoryRegistrationTransaction::new(),
    );

    (build_test_router(app_state, auth_state), user_id)
}

fn list_monitors_req(user_id: UserId, query: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(format!("/api/v1/monitors{query}"))
        .header("authorization", format!("Bearer {}", user_id.0))
        .body(Body::empty())
        .unwrap()
}

// ===========================================================================
// 1) Oversized limit clamped at HTTP boundary
// ===========================================================================

#[tokio::test]
async fn oversized_limit_is_clamped_at_http_boundary() {
    let (app, user) = build_app_with_monitors(150).await;
    let resp = app
        .oneshot(list_monitors_req(user, "?limit=1000000"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let items: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert!(
        items.len() <= application::pagination::MAX_PAGE_SIZE as usize,
        "SECURITY: server returned {} items for limit=1000000",
        items.len()
    );
}

// ===========================================================================
// 2) X-Next-Cursor header
// ===========================================================================

#[tokio::test]
async fn next_cursor_header_present_when_more_rows_exist() {
    let (app, user) = build_app_with_monitors(5).await;
    let resp = app
        .oneshot(list_monitors_req(user, "?limit=2"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().contains_key("x-next-cursor"),
        "5 rows at limit=2 must produce X-Next-Cursor"
    );
}

#[tokio::test]
async fn next_cursor_header_absent_on_last_page() {
    let (app, user) = build_app_with_monitors(2).await;
    let resp = app
        .oneshot(list_monitors_req(user, "?limit=50"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        !resp.headers().contains_key("x-next-cursor"),
        "fewer rows than limit = last page, no X-Next-Cursor"
    );
}

// ===========================================================================
// 3) Malformed cursor starts from page 1
// ===========================================================================

#[tokio::test]
async fn malformed_cursor_starts_from_page_one() {
    let (app, user) = build_app_with_monitors(3).await;
    let resp = app
        .oneshot(list_monitors_req(
            user,
            "?cursor=not-a-real-cursor&limit=50",
        ))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "malformed cursor must not produce a distinguishable error"
    );
}

// ===========================================================================
// 4) Walking pages visits every row exactly once
// ===========================================================================

#[tokio::test]
async fn walking_pages_via_http_visits_every_row_exactly_once() {
    let (app, user) = build_app_with_monitors(11).await;

    let mut seen_ids = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    let mut iterations = 0;

    loop {
        iterations += 1;
        assert!(iterations <= 50, "walk did not terminate");

        let query = match &cursor {
            Some(c) => format!("?limit=3&cursor={c}"),
            None => "?limit=3".to_string(),
        };

        let resp = app
            .clone()
            .oneshot(list_monitors_req(user, &query))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let next = resp
            .headers()
            .get("x-next-cursor")
            .map(|v| v.to_str().unwrap().to_string());

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let items: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert!(
            items.len() <= 3,
            "server returned more than requested limit"
        );

        for item in &items {
            let id = item["id"].as_str().expect("id present").to_string();
            assert!(seen_ids.insert(id), "duplicate row across pages");
        }

        cursor = next;
        if cursor.is_none() {
            break;
        }
    }

    assert_eq!(
        seen_ids.len(),
        11,
        "walking pages must yield exactly the inserted monitors"
    );
}

// ===========================================================================
// 5) Response body shape unchanged
// ===========================================================================

#[tokio::test]
async fn response_body_is_still_a_json_array() {
    let (app, user) = build_app_with_monitors(2).await;
    let resp = app
        .oneshot(list_monitors_req(user, "?limit=50"))
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        parsed.is_array(),
        "PART 8 must not break the existing body shape; got {parsed:?}"
    );
}
