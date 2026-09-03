use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    PasswordHasher, SecretCipher, SecretCipherError, SubscriptionRepository,
    TenantMembershipRepository, TokenService, UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::enums::PlanTier;
use domain::pricing::PricingConfig;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::Role;
use domain::{Subscription, SubscriptionId, TenantMembership, User};
use infra::repositories::in_memory_pricing_config_repository::InMemoryPricingConfigRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    InMemoryRegistrationTransaction, InMemorySubscriptionRepository,
    InMemorySubscriptionUsagePeriodRepository, InMemorySubscriptionUsageTransaction,
    InMemoryTenantMembershipRepository, InMemoryTenantRepository, NoopNotificationService,
    UuidIdGenerator,
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

fn build_test_app(
    membership_repo: InMemoryTenantMembershipRepository,
    pricing_repo: InMemoryPricingConfigRepository,
    subscription_repo: Arc<InMemorySubscriptionRepository>,
) -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();
    let usage_period_repo: Arc<InMemorySubscriptionUsagePeriodRepository> =
        Arc::new(InMemorySubscriptionUsagePeriodRepository::new());
    let usage_transaction = InMemorySubscriptionUsageTransaction::new(
        subscription_repo.clone(),
        usage_period_repo.clone(),
    );

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
    .with_pricing_config_repository(pricing_repo)
    .with_subscription_repository(subscription_repo)
    .with_subscription_usage_period_repository(usage_period_repo)
    .with_subscription_usage_transaction(usage_transaction);

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

#[tokio::test]
async fn member_without_manage_billing_gets_forbidden() {
    let tenant_id = TenantId(Uuid::new_v4());
    let member_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, member_user, tenant_id, Role::Member).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());

    let app = build_test_app(membership_repo, pricing_repo, subscription_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/subscription/usage")
                .header("content-type", "application/json")
                .header("authorization", auth_header(member_user.0))
                .body(Body::from(json!({ "plan_tier": "tier_1" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn owner_without_existing_subscription_gets_not_found() {
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());

    let app = build_test_app(membership_repo, pricing_repo, subscription_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/subscription/usage")
                .header("content-type", "application/json")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::from(json!({ "plan_tier": "tier_1" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn owner_can_update_usage_and_response_reflects_the_subscriptions_own_plan_tier() {
    // SECURITY (H13): the endpoint no longer trusts `plan_tier` from the
    // request body -- it reads the tenant's actual plan tier from the
    // subscription in the database. Here the DB has Tier5, and the body
    // requests a different tier ("tier_1"), to prove the body value is
    // ignored and the response reflects the DB's tier instead.
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());

    let subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Tier5);
    subscription_repo.create(&subscription).await.unwrap();

    let app = build_test_app(membership_repo, pricing_repo, subscription_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/subscription/usage")
                .header("content-type", "application/json")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::from(json!({ "plan_tier": "tier_1" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["plan_tier"], "tier_5");
    assert_eq!(body["monitors"], 500);
    assert_eq!(body["interval_seconds"], 60);
    assert_eq!(body["retention_days"], 90);
    assert_eq!(body["pricing_version"], 1);
}

#[tokio::test]
async fn client_supplied_plan_tier_in_body_is_ignored() {
    // SECURITY (H13, fixed): the endpoint used to trust `plan_tier` from
    // the request body, letting any caller with ManageBilling permission
    // downgrade their own tenant's tier. It now ignores the body entirely
    // and always reflects the subscription's tier as stored in the DB.
    // A garbage tier in the body must NOT cause a 400 -- there is nothing
    // left to validate, since the body is never read.
    let tenant_id = TenantId(Uuid::new_v4());
    let owner_user = UserId(Uuid::new_v4());
    let membership_repo = InMemoryTenantMembershipRepository::new();
    insert_membership(&membership_repo, owner_user, tenant_id, Role::Owner).await;

    let pricing_repo =
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config());
    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
    let subscription = Subscription::new(SubscriptionId::new(), tenant_id, PlanTier::Free);
    subscription_repo.create(&subscription).await.unwrap();

    let app = build_test_app(membership_repo, pricing_repo, subscription_repo);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/subscription/usage")
                .header("content-type", "application/json")
                .header("authorization", auth_header(owner_user.0))
                .body(Body::from(
                    json!({ "plan_tier": "not_a_real_tier" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["plan_tier"], "free");
}
