use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::HelpCategoryRepository;
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher, SecretCipher,
    SecretCipherError, TokenService, UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::entities::{CreateHelpCategory, HelpCategory};
use domain::shared::ids::HelpCategoryId;
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryHelpArticleRepository,
    InMemoryHelpCategoryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    InMemoryRegistrationTransaction, InMemoryTenantMembershipRepository, InMemoryTenantRepository,
    NoopNotificationService, UuidIdGenerator,
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

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
struct NoopUserRepository;
#[async_trait]
impl UserRepository for NoopUserRepository {
    async fn create(
        &self,
        _: &domain::User,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(
        &self,
        _: &domain::User,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: &domain::UserId,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_email(
        &self,
        _: &domain::Email,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_username(
        &self,
        _: &domain::Username,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
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
#[async_trait]
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
#[async_trait]
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
#[tokio::test]
async fn list_help_categories_returns_seeded_categories() {
    let monitor_repo = InMemoryMonitorRepository::new();
    let cat_repo = Arc::new(InMemoryHelpCategoryRepository::new());
    let art_repo = Arc::new(InMemoryHelpArticleRepository::new());

    let cat = HelpCategory::create(CreateHelpCategory {
        id: HelpCategoryId::new(),
        name: "Monitors".into(),
        slug: "monitors".into(),
        description: "Monitor help".into(),
        sort_order: 2,
    })
    .unwrap();
    let cat2 = HelpCategory::create(CreateHelpCategory {
        id: HelpCategoryId::new(),
        name: "Billing".into(),
        slug: "billing-plans".into(),
        description: "Billing".into(),
        sort_order: 1,
    })
    .unwrap();
    cat_repo.create(&cat).await.unwrap();
    cat_repo.create(&cat2).await.unwrap();

    let app_state = AppState::new(
        monitor_repo.clone(),
        NoopCheckRepository,
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_help_category_repository(cat_repo)
    .with_help_article_repository(art_repo);

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        InMemoryTenantMembershipRepository::new(),
        InMemoryRegistrationTransaction::new(),
    );
    let app = build_test_router(app_state, auth_state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/help/categories")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["slug"], "billing-plans");
    assert_eq!(arr[1]["slug"], "monitors");
}
