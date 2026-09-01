use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::TenantMembershipRepository;
use application::ports::{PasswordHasher, TokenService, UserRepository};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::pricing::PricingConfig;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::Role;
use domain::{TenantMembership, User};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::repositories::in_memory_pricing_config_repository::InMemoryPricingConfigRepository;
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
        // id the middleware asks about; role/permission logic in these
        // tests is exercised separately via InMemoryTenantMembershipRepository.
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

fn build_test_app(
    membership_repo: InMemoryTenantMembershipRepository,
    pricing_repo: InMemoryPricingConfigRepository,
) -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();

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
    )
    .with_pricing_config_repository(pricing_repo);

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        membership_repo,
        InMemoryRegistrationTransaction::new(),
    );

    build_test_router(app_state, auth_state)
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

fn valid_pricing_payload(version: u32) -> Value {
    json!({
        "cost_per_check": "0.000001",
        "effective_storage_bytes_per_check": 1024,
        "cost_per_gb_hot": "0.21",
        "cost_per_gb_warm": "0.11",
        "cost_per_gb_cold": "0.04",
        "backup_multiplier": "0.25",
        "markup_multiplier": "2.00",
        "min_interval_seconds": 5,
        "max_monitors": 10000,
        "max_retention_days": 3650,
        "min_paid_monthly_price": "3.00",
        "free_tier": {
            "monitors": 30,
            "interval_seconds": 180,
            "retention_days": 3
        },
        "currency": "USD",
        "avg_billable_network_bytes_per_check": 512,
        "cost_per_gb_egress": "0.12",
        "cost_safety_buffer": "1.20",
        "effective_at": null,
        "reason": format!("test version {version}")
    })
}

// A minimal set of Noop* repos for monitor/check/etc., similar to test_app.rs
// but kept here to avoid importing the whole helper.
struct NoopCheckRepository;
#[async_trait]
impl application::ports::CheckRepository for NoopCheckRepository {
    async fn save(
        &self,
        _: &domain::Check,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::shared::ids::CheckId,
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
        _: TenantId,
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
#[async_trait]
impl application::ports::DiagnosticReportRepository for NoopDiagnosticReportRepository {
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
#[async_trait]
impl application::ports::IncidentRepository for NoopIncidentRepository {
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

async fn insert_membership(
    repo: &InMemoryTenantMembershipRepository,
    user_id: UserId,
    tenant_id: TenantId,
    role: Role,
) {
    let membership = TenantMembership {
        tenant_id,
        user_id,
        role,
        invited_at: chrono::Utc::now(),
        joined_at: Some(chrono::Utc::now()),
    };
    repo.create(&membership).await.unwrap();
}

#[tokio::test]
async fn admin_without_manage_billing_gets_forbidden() {
    let tenant_id = TenantId(Uuid::new_v4());
    let admin_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, admin_user, tenant_id, Role::Admin).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let app = build_test_app(membership_repo, pricing_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/pricing/configs")
                .header("authorization", auth_header(admin_user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn owner_can_list_pricing_configs() {
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let app = build_test_app(membership_repo, pricing_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/pricing/configs")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert!(body.is_array());
    assert_eq!(body[0]["version"], 1);
}

#[tokio::test]
async fn owner_can_get_pricing_config_by_version() {
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let app = build_test_app(membership_repo, pricing_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/pricing/configs/1")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["version"], 1);
}

#[tokio::test]
async fn owner_can_get_active_pricing_config() {
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let app = build_test_app(membership_repo, pricing_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/pricing/configs/active")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["version"], 1);
}

#[tokio::test]
async fn owner_can_create_new_pricing_config_and_audit_log_is_written() {
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let app = build_test_app(membership_repo, pricing_repo);

    // Create version 2
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/pricing/configs")
                .header("content-type", "application/json")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::from(valid_pricing_payload(2).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = read_json(response).await;
    assert_eq!(created["version"], 2);

    // List versions now shows two entries
    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/pricing/configs")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = read_json(list_response).await;
    assert_eq!(list_body.as_array().unwrap().len(), 2);

    // Audit logs should contain one entry for the new version
    let audit_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/admin/pricing/audit-logs")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit_response.status(), StatusCode::OK);
    let audit_body = read_json(audit_response).await;
    assert!(!audit_body.as_array().unwrap().is_empty());
    // The first entry should be for version 2 with field "version"
    assert_eq!(audit_body[0]["version"], 2);
    assert_eq!(audit_body[0]["field_name"], "version");
}

#[tokio::test]
async fn validation_rejects_bad_config() {
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let app = build_test_app(membership_repo, pricing_repo);

    let mut bad_payload = valid_pricing_payload(2);
    bad_payload["max_retention_days"] = json!(50);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/pricing/configs")
                .header("content-type", "application/json")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::from(bad_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
