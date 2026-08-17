use std::sync::Arc;

use application::ports::{
    ApiFlowRepository, CheckRepository, CustomDomainVerifier, DiagnosticReportRepository,
    EscalationPolicyRepository, IncidentRepository, LogoStorage, MaintenanceWindowRepository,
    MonitorRepository, NotificationChannelRepository, NotificationService, ProxyCheckJobRepository,
    StatusPageCache, StatusPageRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::StatusPage;
use domain::shared::ids::{
    EscalationPolicyId, MaintenanceWindowId, MonitorId, StatusPageId, UserId,
};
use domain::{EscalationPolicy, MaintenanceWindow};
use infra::UuidIdGenerator;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub monitor_repository: Arc<dyn MonitorRepository>,
    pub check_repository: Arc<dyn CheckRepository>,
    pub diagnostic_report_repository: Arc<dyn DiagnosticReportRepository>,
    pub incident_repository: Arc<dyn IncidentRepository>,
    pub notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    pub notification_service: Arc<dyn NotificationService>,
    pub api_flow_repository: Arc<dyn ApiFlowRepository>,
    pub proxy_check_job_repository: Arc<dyn ProxyCheckJobRepository>,
    pub maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    pub escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
    pub status_page_repository: Arc<dyn StatusPageRepository>,
    pub custom_domain_verifier: Arc<dyn CustomDomainVerifier>,
    pub status_page_cache: Arc<dyn StatusPageCache>,
    pub logo_storage: Arc<dyn LogoStorage>,
    pub id_generator: UuidIdGenerator,
}

#[derive(Clone, Default)]
struct NoopMaintenanceWindowRepository;

#[async_trait]
impl MaintenanceWindowRepository for NoopMaintenanceWindowRepository {
    async fn create(&self, _window: &MaintenanceWindow) -> RepoResult<()> {
        Ok(())
    }

    async fn delete(&self, _id: MaintenanceWindowId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: MaintenanceWindowId,
        _user_id: UserId,
    ) -> RepoResult<Option<MaintenanceWindow>> {
        Ok(None)
    }

    async fn list_by_monitor_for_user(
        &self,
        _monitor_id: MonitorId,
        _user_id: UserId,
    ) -> RepoResult<Option<Vec<MaintenanceWindow>>> {
        Ok(None)
    }

    async fn list_active_by_monitor(
        &self,
        _monitor_id: MonitorId,
        _now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
pub struct NoopLogoStorage;

#[async_trait]
impl LogoStorage for NoopLogoStorage {
    async fn store_logo(&self, _logo: &domain::LogoImage) -> RepoResult<String> {
        Err(std::io::Error::other("logo storage is not configured").into())
    }
}

#[derive(Clone, Default)]
struct NoopEscalationPolicyRepository;

#[async_trait]
impl EscalationPolicyRepository for NoopEscalationPolicyRepository {
    async fn create(&self, _policy: &EscalationPolicy) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _policy: &EscalationPolicy) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id_for_user(
        &self,
        _id: EscalationPolicyId,
        _user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        Ok(None)
    }

    async fn find_effective(
        &self,
        _monitor_id: MonitorId,
        _user_id: UserId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        Ok(None)
    }

    async fn list_for_user(&self, _user_id: UserId) -> RepoResult<Vec<EscalationPolicy>> {
        Ok(vec![])
    }
}

/// No-op status page repository, used by the test/backward-compatible
/// [`AppState::new`] constructor. Never used in production — see
/// [`AppState::new_with_feature_repos`], which takes a real
/// `PostgresStatusPageRepository`.
#[derive(Clone, Default)]
pub struct NoopStatusPageRepository;

#[async_trait]
impl StatusPageRepository for NoopStatusPageRepository {
    async fn save(&self, _status_page: &StatusPage) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _status_page: &StatusPage) -> RepoResult<()> {
        Ok(())
    }

    async fn delete_for_user(&self, _id: StatusPageId, _user_id: UserId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: StatusPageId,
        _user_id: UserId,
    ) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_all_for_user(&self, _user_id: UserId) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_by_subdomain(&self, _subdomain: &str) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_by_verified_custom_domain(
        &self,
        _custom_domain: &str,
    ) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_pending_custom_domains(&self) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }
}

/// No-op domain verifier. Fails closed: without a real verifier
/// configured, no domain is ever reported as verified.
#[derive(Clone, Default)]
pub struct NoopCustomDomainVerifier;

#[async_trait]
impl CustomDomainVerifier for NoopCustomDomainVerifier {
    async fn verify(&self, _domain: &str, _expected_token: &str) -> RepoResult<bool> {
        Ok(false)
    }
}

/// No-op cache. Always misses, writes are discarded — matches the
/// `StatusPageCache` port's documented fail-open contract, and is what
/// `main.rs` wires up when `REDIS_URL` is not configured.
#[derive(Clone, Default)]
pub struct NoopStatusPageCache;

#[async_trait]
impl StatusPageCache for NoopStatusPageCache {
    async fn get(&self, _key: &str) -> RepoResult<Option<String>> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> RepoResult<()> {
        Ok(())
    }

    async fn invalidate(&self, _key: &str) -> RepoResult<()> {
        Ok(())
    }
}

impl AppState {
    /// Test / backward-compatible constructor.
    ///
    /// Uses in-memory no-op maintenance-window, escalation-policy, and
    /// status-page repositories (plus a no-op domain verifier and cache)
    /// so existing tests don't need to provide real ones.
    pub fn new<M, C, D, I, N, S, A, P>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        id_generator: UuidIdGenerator,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        D: DiagnosticReportRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
        A: ApiFlowRepository + 'static,
        P: ProxyCheckJobRepository + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            diagnostic_report_repository: Arc::new(diagnostic_report_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            api_flow_repository: Arc::new(api_flow_repository),
            proxy_check_job_repository: Arc::new(proxy_check_job_repository),
            maintenance_window_repository: Arc::new(NoopMaintenanceWindowRepository),
            escalation_policy_repository: Arc::new(NoopEscalationPolicyRepository),
            status_page_repository: Arc::new(NoopStatusPageRepository),
            custom_domain_verifier: Arc::new(NoopCustomDomainVerifier),
            status_page_cache: Arc::new(NoopStatusPageCache),
            logo_storage: Arc::new(NoopLogoStorage),
            id_generator,
        }
    }

    /// Production constructor with real feature repositories.
    ///
    /// `status_page_cache` is taken as an already-constructed
    /// `Arc<dyn StatusPageCache>` (rather than a generic parameter, like
    /// every other argument here) because the caller (`main.rs`) needs to
    /// branch at runtime on whether `REDIS_URL` is configured, choosing
    /// between `RedisStatusPageCache` and `NoopStatusPageCache` — that
    /// branch has to happen before this function is called.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_feature_repos<M, C, D, I, N, S, A, P, MW, EP, SP, CDV>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        maintenance_window_repository: MW,
        escalation_policy_repository: EP,
        status_page_repository: SP,
        custom_domain_verifier: CDV,
        status_page_cache: Arc<dyn StatusPageCache>,
        logo_storage: Arc<dyn LogoStorage>,
        id_generator: UuidIdGenerator,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        D: DiagnosticReportRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
        A: ApiFlowRepository + 'static,
        P: ProxyCheckJobRepository + 'static,
        MW: MaintenanceWindowRepository + 'static,
        EP: EscalationPolicyRepository + 'static,
        SP: StatusPageRepository + 'static,
        CDV: CustomDomainVerifier + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            diagnostic_report_repository: Arc::new(diagnostic_report_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            api_flow_repository: Arc::new(api_flow_repository),
            proxy_check_job_repository: Arc::new(proxy_check_job_repository),
            maintenance_window_repository: Arc::new(maintenance_window_repository),
            escalation_policy_repository: Arc::new(escalation_policy_repository),
            status_page_repository: Arc::new(status_page_repository),
            custom_domain_verifier: Arc::new(custom_domain_verifier),
            status_page_cache,
            logo_storage,
            id_generator,
        }
    }
}
