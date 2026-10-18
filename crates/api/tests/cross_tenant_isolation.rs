//! Cross-tenant isolation integration tests.
//!
//! Covers the API-key surface end-to-end through the HTTP router: two
//! distinct `user_id`s land in two distinct tenants (see
//! `InMemoryTenantMembershipRepository::list_by_user`'s fallback —
//! an unseeded user is auto-provisioned into its own brand-new tenant),
//! so a fresh `Uuid::new_v4()` per test is sufficient to simulate two
//! separate tenants without explicit membership seeding.
//!
//! Other tenant-scoped resources (notification channels, API flows,
//! proxy-check jobs, escalation policies) are already guarded at the
//! application layer by `AuthorizationPolicy::same_tenant` (see e.g.
//! `DeleteNotificationChannelUseCase`, `UpdateApiFlowUseCase`,
//! `GetProxyCheckJobUseCase`) but do not yet have HTTP-layer
//! cross-tenant tests here — see the test-suite notes for
//! why, and follow this file's pattern to add them.

use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher, TokenService,
    UserRepository,
};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId};
use domain::{Check, DiagnosticReport, Incident, User};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryApiKeyRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    NoopNotificationService, UuidIdGenerator,
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

fn build_test_app(tenant_a_user: Uuid, tenant_b_user: Uuid) -> axum::Router {
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
    )
    .with_api_key_repository(InMemoryApiKeyRepository::new());

    let tenant_repository = InMemoryTenantRepository::new();
    let tenant_membership_repository = InMemoryTenantMembershipRepository::new();

    let tenant_a = tenant_repository.seed_for_user(domain::UserId(tenant_a_user));
    let tenant_b = tenant_repository.seed_for_user(domain::UserId(tenant_b_user));

    tenant_membership_repository.seed_owner(tenant_a, domain::UserId(tenant_a_user));
    tenant_membership_repository.seed_owner(tenant_b, domain::UserId(tenant_b_user));

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

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Tenant A creates an API key; Tenant B must not be able to delete it,
/// and must get the SAME response (404) whether the key doesn't exist at
/// all or belongs to a different tenant — no timing/response oracle for
/// "this id exists but isn't yours".
#[tokio::test]
async fn cross_tenant_cannot_delete_another_tenants_api_key() {
    let tenant_a_user = Uuid::new_v4();
    let tenant_b_user = Uuid::new_v4();
    let app = build_test_app(tenant_a_user, tenant_b_user);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(tenant_a_user))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"name": "Tenant A key", "scopes": ["monitor:read"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = body_json(create_response).await;
    let key_id = created["id"].as_str().unwrap();

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/api-keys/{key_id}"))
                .header("Authorization", auth_header(tenant_b_user))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        delete_response.status(),
        StatusCode::NOT_FOUND,
        "a different tenant must not be able to revoke another tenant's api key"
    );

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(tenant_a_user))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(list_response).await;
    let keys = body.as_array().unwrap();
    assert_eq!(
        keys.len(),
        1,
        "the cross-tenant delete attempt must not have actually removed the key"
    );
    assert_eq!(keys[0]["id"], key_id);
}

/// Tenant B's key listing must never include Tenant A's keys, even when
/// both tenants have created keys.
#[tokio::test]
async fn cross_tenant_api_key_listing_is_isolated() {
    let tenant_a_user = Uuid::new_v4();
    let tenant_b_user = Uuid::new_v4();
    let app = build_test_app(tenant_a_user, tenant_b_user);

    for (user, name) in [(tenant_a_user, "A's key"), (tenant_b_user, "B's key")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/api-keys")
                    .header("Authorization", auth_header(user))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({"name": name, "scopes": ["monitor:read"]}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/api-keys")
                .header("Authorization", auth_header(tenant_b_user))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = body_json(list_response).await;
    let keys = body.as_array().unwrap();
    assert_eq!(
        keys.len(),
        1,
        "tenant B must only ever see its own keys, never tenant A's"
    );
    assert_eq!(keys[0]["name"], "B's key");
}
