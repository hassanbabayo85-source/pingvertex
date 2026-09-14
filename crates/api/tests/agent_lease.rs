use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{CheckRepository, MonitorRepository};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum_server_mtls::PeerCertificates;
use chrono::{Duration, Utc};
use domain::shared::ids::{CheckId, MonitorId, TenantId};
use domain::{Check, Monitor, RegionCode};
use infra::{
    InMemoryAgentLeaseRepository, InMemoryAuditLogEntryRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryRegistrationTransaction, InMemoryTenantMembershipRepository, InMemoryTenantRepository,
    NoopNotificationService, UuidIdGenerator,
};
use rustls_pki_types::CertificateDer;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

fn peer_certificates_with_cn(cn: &str) -> PeerCertificates {
    let mut params =
        rcgen::CertificateParams::new(Vec::<String>::new()).expect("empty SAN list is valid");
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);

    let key_pair = rcgen::KeyPair::generate().expect("generate key pair");
    let cert = params
        .self_signed(&key_pair)
        .expect("self-sign test certificate");

    let cert_der = CertificateDer::from(cert.der().to_vec());
    PeerCertificates::new(vec![cert_der])
}

#[derive(Clone, Default)]
struct MockCheckRepository {
    saved_checks: Arc<Mutex<Vec<Check>>>,
}

impl MockCheckRepository {}

#[async_trait]
impl CheckRepository for MockCheckRepository {
    async fn save(&self, check: &Check) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.saved_checks.lock().unwrap().push(check.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: domain::shared::ids::CheckId,
        _: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn list_checks(
        &self,
        _limit: u32,
        _offset: u32,
        _tenant_id: TenantId,
    ) -> Result<Vec<Check>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn delete_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        Ok(0)
    }
}

#[derive(Clone, Default)]
struct NoopDiagnosticReportRepository;

#[async_trait]
impl application::ports::DiagnosticReportRepository for NoopDiagnosticReportRepository {
    async fn save(
        &self,
        _report: &domain::DiagnosticReport,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_check_id(
        &self,
        _check_id: domain::shared::ids::CheckId,
    ) -> Result<Option<domain::DiagnosticReport>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct NoopIncidentRepository;

#[async_trait]
impl application::ports::IncidentRepository for NoopIncidentRepository {
    async fn save(
        &self,
        _incident: &domain::Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(
        &self,
        _incident: &domain::Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _id: domain::shared::ids::IncidentId,
    ) -> Result<Option<domain::Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_open_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Option<domain::Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<domain::Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopUserRepository;

#[async_trait]
impl application::ports::UserRepository for NoopUserRepository {
    async fn create(
        &self,
        _user: &domain::User,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(
        &self,
        _user: &domain::User,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _id: &domain::UserId,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_email(
        &self,
        _email: &domain::Email,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_username(
        &self,
        _username: &domain::Username,
    ) -> Result<Option<domain::User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn delete(
        &self,
        _id: &domain::UserId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn email_exists(
        &self,
        _email: &domain::Email,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
    async fn username_exists(
        &self,
        _username: &domain::Username,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
}

#[derive(Clone, Default)]
struct NoopPasswordHasher;

#[async_trait]
impl application::ports::PasswordHasher for NoopPasswordHasher {
    async fn hash(
        &self,
        _password: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(String::new())
    }
    async fn verify(
        &self,
        _password: &str,
        _hash: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct NoopTokenService;

#[async_trait]
impl application::ports::TokenService for NoopTokenService {
    async fn generate(
        &self,
        _user_id: domain::UserId,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(String::new())
    }
    async fn verify(
        &self,
        _token: &str,
    ) -> Result<application::ports::VerifiedToken, Box<dyn std::error::Error + Send + Sync>> {
        Ok(application::ports::VerifiedToken {
            user_id: domain::UserId::from_uuid(uuid::Uuid::nil()),
            issued_at: 0,
        })
    }
}

#[derive(Clone, Default)]
struct NoopSecretCipher;

impl application::ports::SecretCipher for NoopSecretCipher {
    fn encrypt_bytes(
        &self,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, application::ports::SecretCipherError> {
        Ok(plaintext.to_vec())
    }
    fn decrypt_bytes(
        &self,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, application::ports::SecretCipherError> {
        Ok(ciphertext.to_vec())
    }
}

fn build_monitor_with_regions(regions: Vec<RegionCode>) -> Monitor {
    let mut monitor = domain::MonitorBuilder::new(
        MonitorId::new(),
        domain::shared::ids::UserId::new(),
        domain::shared::ids::TenantId::new(),
    )
    .name(domain::MonitorName::new("Test Monitor").unwrap())
    .url(domain::Url::new("http://example.com").unwrap())
    .method(domain::HttpMethod::Get)
    .expected_status(domain::ExpectedStatus::new(200).unwrap())
    .interval(domain::CheckInterval::new(60).unwrap())
    .timeout(domain::Timeout::new(5).unwrap())
    .build();
    monitor.regions = regions;
    monitor
}

async fn build_app_with_lease_repo(
    monitor: Monitor,
    lease_repo: Arc<dyn application::ports::AgentLeaseRepository>,
) -> (axum::Router, Arc<MockCheckRepository>) {
    let monitor_repo = InMemoryMonitorRepository::new();
    monitor_repo.save(&monitor).await.unwrap();

    let check_repo = Arc::new(MockCheckRepository::default());
    let state = AppState::new(
        monitor_repo.clone(),
        check_repo.clone(),
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo),
        NoopNotificationService::new(),
        infra::InMemoryApiFlowRepository::new(),
        infra::InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_agent_lease_repository(lease_repo)
    .with_agent_registry({
        // PART 5 — yadda test ɗin yake aiki: agent-1 a us-east.
        let reg = infra::InMemoryAgentRegistry::new();
        reg.register(domain::Agent::new("agent-1", "us-east"));
        reg
    });

    let auth_state = AuthState::new(
        NoopUserRepository,
        NoopPasswordHasher,
        NoopTokenService,
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        InMemoryTenantMembershipRepository::new(),
        InMemoryRegistrationTransaction::new(),
    );

    (build_test_router(state, auth_state), check_repo)
}

fn submit_request(check_id: uuid::Uuid, monitor_id: uuid::Uuid) -> Request<Body> {
    let body = serde_json::json!({
        "check_id": check_id,
        "monitor_id": monitor_id,
        "region": "us-east",
        "success": true,
        "status_code": 200,
        "latency_ms": 12,
        "error": null,
        "attempted": true,
        "country_code": null
    });

    Request::builder()
        .method("POST")
        .uri("/internal/v1/agents/agent-1/results")
        .extension(peer_certificates_with_cn("agent-1"))
        .header("X-Agent-Region", "us-east")
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn stale_result_for_expired_lease_is_rejected_and_does_not_save_check() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let check_id = CheckId::new();
    let monitor_id = monitor.id;

    let lease_repo = Arc::new(InMemoryAgentLeaseRepository::new());
    lease_repo.seed_lease(
        check_id,
        monitor_id,
        RegionCode::new("us-east").unwrap(),
        "agent-1",
        Utc::now() - Duration::seconds(1),
        None,
    );

    let (app, _check_repo) = build_app_with_lease_repo(monitor, lease_repo.clone()).await;
    let response = app
        .oneshot(submit_request(check_id.0, monitor_id.0))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(lease_repo.saved_check_count(), 0);
}

#[tokio::test]
async fn duplicate_result_for_completed_lease_is_idempotent_and_does_not_save_check() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let check_id = CheckId::new();
    let monitor_id = monitor.id;

    let lease_repo = Arc::new(InMemoryAgentLeaseRepository::new());
    lease_repo.seed_lease(
        check_id,
        monitor_id,
        RegionCode::new("us-east").unwrap(),
        "agent-1",
        Utc::now() + Duration::seconds(60),
        Some(Utc::now()),
    );

    let (app, _check_repo) = build_app_with_lease_repo(monitor, lease_repo.clone()).await;
    let response = app
        .oneshot(submit_request(check_id.0, monitor_id.0))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(lease_repo.saved_check_count(), 0);
}

#[tokio::test]
async fn accepted_result_saves_check_exactly_once_through_lease_repository() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let check_id = CheckId::new();
    let monitor_id = monitor.id;

    let lease_repo = Arc::new(InMemoryAgentLeaseRepository::new());
    lease_repo.seed_lease(
        check_id,
        monitor_id,
        RegionCode::new("us-east").unwrap(),
        "agent-1",
        Utc::now() + Duration::seconds(60),
        None,
    );

    let (app, _check_repo) = build_app_with_lease_repo(monitor, lease_repo.clone()).await;
    let response = app
        .oneshot(submit_request(check_id.0, monitor_id.0))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(lease_repo.saved_check_count(), 1);
}
