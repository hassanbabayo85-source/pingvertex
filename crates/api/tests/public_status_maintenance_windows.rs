use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, MaintenanceWindowRepository,
    PasswordHasher, TokenService, UserRepository,
};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use domain::shared::ids::MaintenanceWindowId;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{Check, DiagnosticReport, Incident, MaintenanceWindow, User};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryLogoStorage, InMemoryMaintenanceWindowRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, InMemoryStatusPageRepository, NoopNotificationService,
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
        _: &UserId,
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

    async fn delete(&self, _: &UserId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        user_id: UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(user_id.0.to_string())
    }

    async fn verify(
        &self,
        token: &str,
    ) -> Result<application::ports::VerifiedToken, Box<dyn std::error::Error + Send + Sync>> {
        let user_id = Uuid::parse_str(token)?;
        Ok(application::ports::VerifiedToken {
            user_id: UserId(user_id),
            issued_at: 0,
        })
    }
}

/// Builds a test app whose `status_page_repository` and
/// `maintenance_window_repository` are both real in-memory repos (instead
/// of the `AppState::new` defaults), so a test can seed both a status page
/// and maintenance windows and see them reflected in the public response.
fn build_test_app(
    status_page_repo: InMemoryStatusPageRepository,
    maintenance_window_repo: InMemoryMaintenanceWindowRepository,
) -> axum::Router {
    let monitor_repository = InMemoryMonitorRepository::new();

    let mut state = AppState::new(
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

    state.status_page_repository = Arc::new(status_page_repo);
    state.logo_storage = Arc::new(InMemoryLogoStorage::new());
    state.maintenance_window_repository = Arc::new(maintenance_window_repo);

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        InMemoryTenantMembershipRepository::new(),
        InMemoryRegistrationTransaction::new(),
    );

    build_test_router(state, auth_state)
}

fn status_page_payload(name: &str, subdomain: &str, monitor_ids: Vec<String>) -> Value {
    json!({
        "name": name,
        "subdomain": subdomain,
        "monitor_ids": monitor_ids,
    })
}

fn auth_header(user_id: Uuid) -> String {
    format!("Bearer {user_id}")
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn public_status_page_includes_maintenance_windows_field() {
    let status_page_repo = InMemoryStatusPageRepository::new();
    let maintenance_window_repo = InMemoryMaintenanceWindowRepository::new();

    let app = build_test_app(status_page_repo, maintenance_window_repo.clone());
    let user_id = Uuid::new_v4();

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/status-pages")
                .header("content-type", "application/json")
                .header("authorization", auth_header(user_id))
                .body(Body::from(
                    status_page_payload("Maint Page", "maintpage", vec![]).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/public/status/maintpage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    assert_eq!(body["monitors"].as_array().unwrap().len(), 0);

    let window = MaintenanceWindow::new(
        MaintenanceWindowId::new(),
        MonitorId::new(),
        TenantId::new(),
        Utc::now() - Duration::minutes(5),
        Utc::now() + Duration::minutes(55),
        "Scheduled DB upgrade".to_string(),
    )
    .unwrap();
    maintenance_window_repo.create(&window).await.unwrap();

    let active = maintenance_window_repo
        .list_active_by_monitor(window.monitor_id, Utc::now())
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].reason, "Scheduled DB upgrade");
}

#[tokio::test]
async fn maintenance_window_repository_excludes_inactive_windows() {
    let maintenance_window_repo = InMemoryMaintenanceWindowRepository::new();
    let monitor_id = MonitorId::new();

    let past_window = MaintenanceWindow::new(
        MaintenanceWindowId::new(),
        monitor_id,
        TenantId::new(),
        Utc::now() - Duration::hours(3),
        Utc::now() - Duration::hours(1),
        "Already finished".to_string(),
    )
    .unwrap();
    maintenance_window_repo.create(&past_window).await.unwrap();

    let future_window = MaintenanceWindow::new(
        MaintenanceWindowId::new(),
        monitor_id,
        TenantId::new(),
        Utc::now() + Duration::hours(1),
        Utc::now() + Duration::hours(3),
        "Not started yet".to_string(),
    )
    .unwrap();
    maintenance_window_repo
        .create(&future_window)
        .await
        .unwrap();

    let active = maintenance_window_repo
        .list_active_by_monitor(monitor_id, Utc::now())
        .await
        .unwrap();

    assert!(active.is_empty());
}
