//! PART 5 — Agent Trust Boundary tests.

use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, PasswordHasher, SecretCipher,
    SecretCipherError, TokenService, UserRepository,
};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use axum_server_mtls::PeerCertificates;
use domain::pricing::PricingConfig;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, TenantId, UserId};
use domain::{Agent, AgentStatus, Check, DiagnosticReport, Incident, User};
use infra::{
    InMemoryAgentLeaseRepository, InMemoryAgentRegistry, InMemoryApiFlowRepository,
    InMemoryAuditLogEntryRepository, InMemoryMonitorRepository,
    InMemoryNotificationChannelRepository, InMemoryPAYGPricingRepository,
    InMemoryPricingConfigRepository, InMemoryProxyCheckJobRepository,
    InMemoryRegistrationTransaction, InMemoryTenantMembershipRepository, InMemoryTenantRepository,
    InMemoryUsageEventRepository, InMemoryWalletRepository, NoopNotificationService,
    UuidIdGenerator,
};
use rustls_pki_types::CertificateDer;
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

#[derive(Clone, Default)]
struct NoopUserRepo;
#[async_trait]
impl UserRepository for NoopUserRepo {
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
        _: &str,
    ) -> Result<application::ports::VerifiedToken, Box<dyn std::error::Error + Send + Sync>> {
        Err("not used".into())
    }
}

async fn build_app(registry: Arc<InMemoryAgentRegistry>) -> axum::Router {
    let pricing_repo = Arc::new(
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config()),
    );
    let app_state = AppState::new(
        InMemoryMonitorRepository::new(),
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
    .with_agent_registry((*registry).clone());

    let auth_state = AuthState::new(
        NoopUserRepo,
        NoopHash,
        NoopTok,
        UuidIdGenerator::new(),
        InMemoryTenantRepository::new(),
        InMemoryTenantMembershipRepository::new(),
        InMemoryRegistrationTransaction::new(),
    );
    build_test_router(app_state, auth_state)
}

fn peer_cert(cn: &str) -> PeerCertificates {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("empty SAN list");
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);
    let kp = rcgen::KeyPair::generate().expect("key pair");
    let cert = params.self_signed(&kp).expect("self-sign");
    PeerCertificates::new(vec![CertificateDer::from(cert.der().to_vec())])
}

fn claim_req(agent_id: &str, cert_cn: &str, region_header: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri(format!("/internal/v1/agents/{agent_id}/checks/claim"))
        .header("content-type", "application/json");
    if let Some(r) = region_header {
        b = b.header("X-Agent-Region", r);
    }
    let mut req = b.body(Body::empty()).unwrap();
    req.extensions_mut().insert(peer_cert(cert_cn));
    req
}

#[tokio::test]
async fn valid_agent_with_correct_region_succeeds() {
    let reg = Arc::new(InMemoryAgentRegistry::new());
    reg.register(Agent::new("agent-1", "us-east"));
    let app = build_app(reg).await;
    let resp = app
        .oneshot(claim_req("agent-1", "agent-1", Some("us-east")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn valid_agent_with_forged_region_header_fails() {
    let reg = Arc::new(InMemoryAgentRegistry::new());
    reg.register(Agent::new("agent-1", "us-east"));
    let app = build_app(reg).await;
    let resp = app
        .oneshot(claim_req("agent-1", "agent-1", Some("eu-west")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn valid_cert_for_agent_a_using_url_agent_b_fails() {
    let reg = Arc::new(InMemoryAgentRegistry::new());
    reg.register(Agent::new("agent-1", "us-east"));
    reg.register(Agent::new("agent-2", "eu-west"));
    let app = build_app(reg).await;
    let resp = app
        .oneshot(claim_req("agent-2", "agent-1", Some("us-east")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_certificate_fails() {
    let reg = Arc::new(InMemoryAgentRegistry::new());
    let app = build_app(reg).await;
    let resp = app
        .oneshot(claim_req("ghost", "ghost", Some("us-east")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoked_agent_fails() {
    let reg = Arc::new(InMemoryAgentRegistry::new());
    reg.register(Agent::new("agent-revoked", "us-east").with_status(AgentStatus::Revoked));
    let app = build_app(reg).await;
    let resp = app
        .oneshot(claim_req("agent-revoked", "agent-revoked", Some("us-east")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
