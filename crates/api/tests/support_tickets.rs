use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher, SecretCipher,
    SecretCipherError, SupportTicketRepository, TenantMembershipRepository, TokenService,
    UserRepository,
};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use domain::entities::CreateSupportTicket;
use domain::enums::{SupportCategory, SupportTicketStatus};
use domain::shared::ids::{SupportTicketId, TenantId, UserId};
use domain::shared::types::Role;
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryProxyCheckJobRepository,
    InMemoryRegistrationTransaction, InMemorySupportTicketRepository,
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
        id: &domain::UserId,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
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
        let user_id = Uuid::parse_str(token)?;
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

fn build_app(
    support_repo: Arc<InMemorySupportTicketRepository>,
    memberships: InMemoryTenantMembershipRepository,
) -> axum::Router {
    let monitor_repo = InMemoryMonitorRepository::new();
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
    .with_support_ticket_repository(support_repo);

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        memberships,
        InMemoryRegistrationTransaction::new(),
    );
    build_test_router(app_state, auth_state)
}

async fn insert_membership(
    repo: &InMemoryTenantMembershipRepository,
    user_id: UserId,
    tenant_id: TenantId,
    role: Role,
) {
    let membership = domain::entities::TenantMembership {
        tenant_id,
        user_id,
        role,
        invited_at: chrono::Utc::now(),
        joined_at: Some(chrono::Utc::now()),
    };
    repo.create(&membership).await.unwrap();
}

#[tokio::test]
async fn tenant_isolation_prevents_cross_tenant_ticket_access() {
    let tenant_a = TenantId(Uuid::new_v4());
    let tenant_b = TenantId(Uuid::new_v4());
    let user_a = UserId(Uuid::new_v4());
    let user_b = UserId(Uuid::new_v4());

    let support_repo = Arc::new(InMemorySupportTicketRepository::new());
    let memberships = InMemoryTenantMembershipRepository::new();
    insert_membership(&memberships, user_a, tenant_a, Role::Owner).await;
    insert_membership(&memberships, user_b, tenant_b, Role::Owner).await;

    let ticket = domain::entities::SupportTicket::create(CreateSupportTicket {
        id: SupportTicketId::new(),
        ticket_number: "PV-TEST0001".into(),
        tenant_id: tenant_a,
        created_by: user_a,
        subject: "Ticket A".into(),
        category: SupportCategory::Account,
        status: SupportTicketStatus::Open,
    })
    .unwrap();
    support_repo.create_ticket(&ticket).await.unwrap();

    let app = build_app(support_repo.clone(), memberships);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/support/tickets/{}", ticket.id.0))
                .header("authorization", format!("Bearer {}", user_b.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_ticket_and_reply_flow() {
    let tenant = TenantId(Uuid::new_v4());
    let user = UserId(Uuid::new_v4());
    let support_repo = Arc::new(InMemorySupportTicketRepository::new());
    let memberships = InMemoryTenantMembershipRepository::new();
    insert_membership(&memberships, user, tenant, Role::Owner).await;

    let app = build_app(support_repo.clone(), memberships);

    // Create ticket
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/support/tickets")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", user.0))
                .body(Body::from(
                    json!({"subject": "Help", "category": "account"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let ticket_id = json["id"].as_str().unwrap().to_string();

    // Reply
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/support/tickets/{}/messages", ticket_id))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", user.0))
                .body(Body::from(json!({"message": "Please help me"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Get ticket with messages
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/support/tickets/{}", ticket_id))
                .header("authorization", format!("Bearer {}", user.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    // Response is a tuple: (ticket, messages)
    let messages = json[1].as_array().unwrap();
    assert_eq!(messages.len(), 1);
}
