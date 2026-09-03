use api::{auth_state::AuthState, router::build_test_router, state::AppState};
use application::ports::{
    CheckRepository, DiagnosticReportRepository, IncidentRepository, MonitorRepository,
    PasswordHasher, TokenService, UserRepository,
};
use application::ports::{SecretCipher, SecretCipherError};
use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum_server_mtls::PeerCertificates;
use domain::shared::ids::TenantId;
use domain::shared::ids::{CheckId, IncidentId, MonitorId, UserId};
use domain::{
    Check, CheckInterval, DiagnosticReport, ExpectedStatus, HttpMethod, Incident, Monitor,
    MonitorBuilder, MonitorName, RegionCode, Timeout, Url, User,
};
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryRegistrationTransaction;
use infra::InMemoryTenantMembershipRepository;
use infra::InMemoryTenantRepository;
use infra::{
    InMemoryApiFlowRepository, InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryProxyCheckJobRepository, NoopNotificationService, UuidIdGenerator,
};
use rustls_pki_types::CertificateDer;
use std::sync::Arc;
use tower::ServiceExt;

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
/// Builds a `PeerCertificates` extension carrying a freshly-generated
/// self-signed certificate whose Common Name (CN) is `cn`.
///
/// This is what `axum-server-mtls`'s `MtlsAcceptor` would inject into a
/// request's extensions after a real mTLS handshake completes — here we
/// skip the network/TLS layer entirely (this is a `tower::ServiceExt::oneshot`
/// in-process test, there is no real TCP/TLS connection) and attach the
/// extension directly to the test `Request`, exercising the exact same
/// `agent_auth` code path (`PeerCertificates::leaf_cn()` +
/// `is_present()`) that runs in production.
///
/// SEC: this replaces the old `AGENT_SECRET` shared-secret Bearer-token
/// test fallback, which has been removed from `agent_auth.rs` entirely —
/// see that file's doc comment. A production authentication code path
/// should never contain a bypass that exists only so tests don't have to
/// do this; this is the honest way to test it instead.
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
    saved_checks: std::sync::Arc<std::sync::Mutex<Vec<Check>>>,
}

impl MockCheckRepository {
    fn save_count(&self) -> usize {
        self.saved_checks.lock().unwrap().len()
    }
}

#[async_trait]
impl CheckRepository for MockCheckRepository {
    async fn save(&self, check: &Check) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.saved_checks.lock().unwrap().push(check.clone());
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: CheckId,
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
        _user_id: TenantId,
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
impl DiagnosticReportRepository for NoopDiagnosticReportRepository {
    async fn save(
        &self,
        _report: &DiagnosticReport,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_check_id(
        &self,
        _check_id: CheckId,
    ) -> Result<Option<DiagnosticReport>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct NoopIncidentRepository;

#[async_trait]
impl IncidentRepository for NoopIncidentRepository {
    async fn save(
        &self,
        _incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn update(
        &self,
        _incident: &Incident,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: IncidentId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn find_open_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Option<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn list_by_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> Result<Vec<Incident>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopUserRepository;

#[async_trait]
impl UserRepository for NoopUserRepository {
    async fn create(&self, _user: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn update(&self, _user: &User) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _id: &domain::UserId,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_email(
        &self,
        _email: &domain::Email,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
    async fn find_by_username(
        &self,
        _username: &domain::Username,
    ) -> Result<Option<User>, Box<dyn std::error::Error + Send + Sync>> {
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
impl PasswordHasher for NoopPasswordHasher {
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
impl TokenService for NoopTokenService {
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

fn build_monitor_with_regions(regions: Vec<RegionCode>) -> Monitor {
    let mut monitor = MonitorBuilder::new(MonitorId::new(), UserId::new(), TenantId::new())
        .name(MonitorName::new("Test Monitor").unwrap())
        .url(Url::new("http://example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build();
    monitor.regions = regions;
    monitor
}

async fn build_test_app(
    monitors: Vec<Monitor>,
) -> (axum::Router, std::sync::Arc<MockCheckRepository>) {
    let monitor_repo = InMemoryMonitorRepository::new();
    let check_repo = std::sync::Arc::new(MockCheckRepository::default());

    for monitor in &monitors {
        monitor_repo.save(monitor).await.unwrap();
    }

    let state = AppState::new(
        monitor_repo.clone(),
        check_repo.clone(),
        NoopDiagnosticReportRepository,
        NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    );

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

fn internal_agent_request(
    method: &str,
    uri: &str,
    peer_certs: Option<PeerCertificates>,
    region: Option<&str>,
    body: Body,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(region) = region {
        builder = builder.header("X-Agent-Region", region);
    }
    if let Some(peer_certs) = peer_certs {
        builder = builder.extension(peer_certs);
    }
    builder.body(body).unwrap()
}

#[tokio::test]
async fn claim_checks_with_valid_mtls_cert_and_region_returns_ok() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let (app, _check_repo) = build_test_app(vec![monitor.clone()]).await;

    let response = app
        .clone()
        .oneshot(internal_agent_request(
            "POST",
            "/internal/v1/agents/agent-1/checks/claim",
            Some(peer_certificates_with_cn("agent-1")),
            Some("us-east"),
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["checks"].is_array());
    assert_eq!(json["checks"].as_array().unwrap().len(), 1);
    assert_eq!(json["checks"][0]["region"], "us-east");
    assert_eq!(json["checks"][0]["monitor"]["id"], monitor.id.0.to_string());
}

#[tokio::test]
async fn claim_checks_without_peer_certificate_returns_unauthorized() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let (app, _check_repo) = build_test_app(vec![monitor]).await;

    // No PeerCertificates extension attached at all — this is the
    // "connection didn't go through mTLS" case. Must be UNAUTHORIZED with
    // no fallback of any kind (this is the regression test for the removed
    // AGENT_SECRET bypass: there is now no header, token, or anything else
    // that can substitute for a real client certificate).
    let response = app
        .clone()
        .oneshot(internal_agent_request(
            "POST",
            "/internal/v1/agents/agent-1/checks/claim",
            None,
            Some("us-east"),
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn claim_checks_with_cn_not_matching_path_agent_id_returns_forbidden() {
    // The certificate itself is genuine and present (so this test is
    // distinct from the UNAUTHORIZED case above), but its CN ("agent-2")
    // doesn't match the {agent_id} segment in the URL ("agent-1"). This
    // exercises the defense-in-depth check that stops an agent with a
    // valid certificate from acting on another agent's behalf — this path
    // was never covered by a test before this change.
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let (app, _check_repo) = build_test_app(vec![monitor]).await;

    let response = app
        .clone()
        .oneshot(internal_agent_request(
            "POST",
            "/internal/v1/agents/agent-1/checks/claim",
            Some(peer_certificates_with_cn("agent-2")),
            Some("us-east"),
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn claim_checks_with_invalid_region_header_returns_bad_request() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let (app, _check_repo) = build_test_app(vec![monitor]).await;

    let response = app
        .clone()
        .oneshot(internal_agent_request(
            "POST",
            "/internal/v1/agents/agent-1/checks/claim",
            Some(peer_certificates_with_cn("agent-1")),
            Some("invalid-region"),
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn submit_result_with_valid_payload_returns_accepted() {
    let monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let (app, check_repo) = build_test_app(vec![monitor.clone()]).await;

    let claim_response = app
        .clone()
        .oneshot(internal_agent_request(
            "POST",
            "/internal/v1/agents/agent-1/checks/claim",
            Some(peer_certificates_with_cn("agent-1")),
            Some("us-east"),
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(claim_response.status(), StatusCode::OK);

    let claim_body = axum::body::to_bytes(claim_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let claim_json: serde_json::Value = serde_json::from_slice(&claim_body).unwrap();
    let check_id = claim_json["checks"][0]["check_id"].as_str().unwrap();
    let monitor_id = claim_json["checks"][0]["monitor"]["id"].as_str().unwrap();

    let submit_body = serde_json::json!({
        "check_id": check_id,
        "monitor_id": monitor_id,
        "region": "us-east",
        "success": true,
        "status_code": 200,
        "latency_ms": 12,
        "error": null
    });

    let submit_request = Request::builder()
        .method("POST")
        .uri("/internal/v1/agents/agent-1/results")
        .extension(peer_certificates_with_cn("agent-1"))
        .header("X-Agent-Region", "us-east")
        .header("Content-Type", "application/json")
        .body(Body::from(submit_body.to_string()))
        .unwrap();

    let submit_response = app.clone().oneshot(submit_request).await.unwrap();

    assert_eq!(submit_response.status(), StatusCode::ACCEPTED);
    assert_eq!(check_repo.save_count(), 1);
}

#[tokio::test]
async fn claim_checks_only_returns_monitors_for_agent_region() {
    let us_monitor = build_monitor_with_regions(vec![RegionCode::new("us-east").unwrap()]);
    let eu_monitor = build_monitor_with_regions(vec![RegionCode::new("eu-west").unwrap()]);
    let (app, _check_repo) = build_test_app(vec![us_monitor.clone(), eu_monitor]).await;

    let response = app
        .clone()
        .oneshot(internal_agent_request(
            "POST",
            "/internal/v1/agents/agent-1/checks/claim",
            Some(peer_certificates_with_cn("agent-1")),
            Some("us-east"),
            Body::empty(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let checks = json["checks"].as_array().unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0]["region"], "us-east");
    assert_eq!(checks[0]["monitor"]["id"], us_monitor.id.0.to_string());
}
