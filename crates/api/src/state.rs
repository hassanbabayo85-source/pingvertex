use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use application::ports::{
    AddOnSubscriptionRepository, AgentLeaseRepository, ApiFlowRepository, ApiKeyRepository,
    AuditLogEntryRepository, BillingAuditLogRepository, BillingWebhookIdempotencyRepository,
    CheckRepository, CheckoutDetails, CheckoutRequest, CheckoutSessionRepository,
    CustomDomainVerifier, DashboardRepository, DiagnosticReportRepository,
    EscalationPolicyRepository, HealthCheckPort, HealthCheckResult, HelpArticleRepository,
    HelpCategoryRepository, IncidentRepository, LogoStorage, MaintenanceWindowRepository,
    MonitorRepository, NotificationChannelRepository, NotificationService, PasswordHasher,
    PaymentProvider, PaymentVerification, PricingConfigRepository, ProxyCheckJobRepository,
    SecretCipher, SmsProviderPricing, StatusPageCache, StatusPageRepository,
    SubscriptionRepository, SubscriptionUsagePeriodRepository, SubscriptionUsageTransaction,
    SupportTicketRepository,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PricingAuditLogEntry;
use domain::Subscription;
use domain::entities::{ApiKey, StatusPage};
use domain::pricing::PricingConfig;
use domain::shared::ids::{ApiKeyId, CheckoutSessionId, MonitorId, StatusPageId, TenantId, UserId};
use domain::{CheckoutSession, EscalationPolicy, MaintenanceWindow};
use infra::{Argon2PasswordHasher, UuidIdGenerator};

use application::dto::DashboardOverview;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Per-token rate limiter for `GET /public/heartbeat/{token}`. Keyed by the
/// raw token string; value is `(window_started_at, requests_in_window)`.
pub type HeartbeatRateLimiter = Arc<tokio::sync::Mutex<HashMap<String, (Instant, u32)>>>;

fn new_heartbeat_rate_limiter() -> HeartbeatRateLimiter {
    Arc::new(tokio::sync::Mutex::new(HashMap::new()))
}

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
    pub secret_cipher: Arc<dyn SecretCipher>,
    pub id_generator: UuidIdGenerator,
    pub heartbeat_rate_limiter: HeartbeatRateLimiter,
    pub audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    pub api_key_repository: Arc<dyn ApiKeyRepository>,
    pub password_hasher: Arc<dyn PasswordHasher>,
    pub subscription_repository: Arc<dyn SubscriptionRepository>,
    pub payment_provider: Arc<dyn PaymentProvider>,
    pub checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
    pub billing_webhook_secret: Option<Arc<str>>,
    pub billing_webhook_idempotency_repository: Arc<dyn BillingWebhookIdempotencyRepository>,
    pub billing_audit_log_repository: Arc<dyn BillingAuditLogRepository>,
    pub pricing_config_repository: Arc<dyn PricingConfigRepository>,
    pub agent_lease_repository: Option<Arc<dyn AgentLeaseRepository>>,
    /// B34: backs `GET /ready`. Defaults to `NoopHealthCheckPort` (always
    /// healthy) until `with_health_check_port` substitutes a real check —
    /// see `main.rs`, which wires `infra::PostgresHealthCheck`.
    pub health_check_port: Arc<dyn HealthCheckPort>,
    /// H6 — Dashboard Overview Aggregated Backend API:
    /// Tenant-scoped, efficient aggregate queries for the dashboard.
    /// Defaults to `NoopDashboardRepository` (returns empty) until
    /// `with_dashboard_repository` substitutes the real Postgres implementation.
    pub dashboard_repository: Arc<dyn DashboardRepository>,
    /// H12 — Proration Service.
    pub subscription_usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
    /// H12 — Proration Service.
    pub subscription_usage_transaction: Arc<dyn SubscriptionUsageTransaction>,
    pub add_on_subscription_repository: Arc<dyn AddOnSubscriptionRepository>,
    pub sms_provider_pricing: Arc<dyn SmsProviderPricing>,
    pub help_category_repository: Arc<dyn HelpCategoryRepository>,
    pub help_article_repository: Arc<dyn HelpArticleRepository>,
    pub support_ticket_repository: Arc<dyn SupportTicketRepository>,
}

/// Default `DashboardRepository` used until a real one is substituted in.
#[derive(Clone, Default)]
struct NoopDashboardRepository;

#[async_trait]
impl DashboardRepository for NoopDashboardRepository {
    async fn get_overview(&self, _tenant_id: TenantId) -> RepoResult<DashboardOverview> {
        Ok(DashboardOverview {
            monitor_summary: application::dto::MonitorSummary {
                total: 0,
                up: 0,
                down: 0,
                disabled: 0,
            },
            monitors: Vec::new(),
            incident_summary: application::dto::IncidentSummaryData {
                open_count: 0,
                recent: Vec::new(),
            },
        })
    }
}

/// Default `ApiKeyRepository` used until a real one is substituted in.
#[derive(Clone, Default)]
struct NoopApiKeyRepository;

#[async_trait]
impl ApiKeyRepository for NoopApiKeyRepository {
    async fn create(&self, _api_key: &ApiKey) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: ApiKeyId) -> RepoResult<Option<ApiKey>> {
        Ok(None)
    }

    async fn find_by_prefix(&self, _key_prefix: &str) -> RepoResult<Option<ApiKey>> {
        Ok(None)
    }

    async fn list_by_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<ApiKey>> {
        Ok(vec![])
    }

    async fn delete(&self, _id: ApiKeyId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn touch_last_used(&self, _id: ApiKeyId, _at: DateTime<Utc>) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopMaintenanceWindowRepository;

#[async_trait]
impl MaintenanceWindowRepository for NoopMaintenanceWindowRepository {
    async fn create(&self, _window: &MaintenanceWindow) -> RepoResult<()> {
        Ok(())
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

    async fn find_effective(
        &self,
        _monitor_id: MonitorId,
        _tenant_id: TenantId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        Ok(None)
    }
}

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

    async fn find_by_id(&self, _id: StatusPageId) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn delete(&self, _id: StatusPageId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_all_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<StatusPage>> {
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

    async fn find_all_referencing_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
pub struct NoopCustomDomainVerifier;

#[async_trait]
impl CustomDomainVerifier for NoopCustomDomainVerifier {
    async fn verify(&self, _domain: &str, _expected_token: &str) -> RepoResult<bool> {
        Ok(false)
    }
}

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

/// B34: default `HealthCheckPort` used until `with_health_check_port`
/// substitutes a real one. Always reports healthy — matches the pattern
/// every other Noop* default in this file follows (safe placeholder,
/// never the production path once `main.rs` wires the real dependency).
#[derive(Clone, Default)]
pub struct NoopHealthCheckPort;

#[async_trait]
impl HealthCheckPort for NoopHealthCheckPort {
    async fn check(&self) -> HealthCheckResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionRepository;

#[async_trait]
impl SubscriptionRepository for NoopSubscriptionRepository {
    async fn create(&self, _subscription: &Subscription) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_tenant_id(&self, _tenant_id: TenantId) -> RepoResult<Option<Subscription>> {
        Ok(None)
    }

    async fn update(&self, _subscription: &Subscription) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopPaymentProvider;

#[async_trait]
impl PaymentProvider for NoopPaymentProvider {
    fn provider_name(&self) -> &'static str {
        "noop"
    }

    async fn create_checkout(&self, _request: &CheckoutRequest) -> RepoResult<CheckoutDetails> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }

    async fn verify_payment(&self, _provider_checkout_id: &str) -> RepoResult<PaymentVerification> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }
}

#[derive(Clone, Default)]
struct NoopCheckoutSessionRepository;

#[async_trait]
impl CheckoutSessionRepository for NoopCheckoutSessionRepository {
    async fn create(&self, _session: &CheckoutSession) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: CheckoutSessionId) -> RepoResult<Option<CheckoutSession>> {
        Ok(None)
    }

    async fn update(&self, _session: &CheckoutSession) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopBillingWebhookIdempotencyRepository;

#[async_trait]
impl BillingWebhookIdempotencyRepository for NoopBillingWebhookIdempotencyRepository {
    async fn try_insert(&self, _provider_event_id: &str) -> RepoResult<bool> {
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct NoopBillingAuditLogRepository;

#[async_trait]
impl BillingAuditLogRepository for NoopBillingAuditLogRepository {
    async fn create(&self, _entry: &domain::BillingAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopPricingConfigRepository;

#[async_trait]
impl PricingConfigRepository for NoopPricingConfigRepository {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>> {
        Ok(None)
    }

    async fn find_by_version(&self, _version: u32) -> RepoResult<Option<PricingConfig>> {
        Ok(None)
    }

    async fn create_version(
        &self,
        _config: &PricingConfig,
        _reason: Option<String>,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn record_audit_log(&self, _entry: &PricingAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        Ok(vec![])
    }

    async fn list_audit_logs(&self) -> RepoResult<Vec<PricingAuditLogEntry>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionUsagePeriodRepository;

#[async_trait]
impl SubscriptionUsagePeriodRepository for NoopSubscriptionUsagePeriodRepository {
    async fn create_period(
        &self,
        _period: &domain::entities::SubscriptionUsagePeriod,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn close_period(
        &self,
        _period_id: domain::shared::ids::SubscriptionUsagePeriodId,
        _end_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn find_open_period(
        &self,
        _subscription_id: domain::shared::ids::SubscriptionId,
    ) -> RepoResult<Option<domain::entities::SubscriptionUsagePeriod>> {
        Ok(None)
    }

    async fn list_periods_for_cycle(
        &self,
        _subscription_id: domain::shared::ids::SubscriptionId,
        _cycle_start: DateTime<Utc>,
        _cycle_end: DateTime<Utc>,
    ) -> RepoResult<Vec<domain::entities::SubscriptionUsagePeriod>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionUsageTransaction;

#[async_trait]
impl SubscriptionUsageTransaction for NoopSubscriptionUsageTransaction {
    async fn change_usage(
        &self,
        _subscription: &Subscription,
        _previous_open_period_id: Option<domain::shared::ids::SubscriptionUsagePeriodId>,
        _new_period: &domain::entities::SubscriptionUsagePeriod,
    ) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopAddOnSubscriptionRepository;
#[async_trait]
impl AddOnSubscriptionRepository for NoopAddOnSubscriptionRepository {
    async fn create(&self, _: &domain::AddOnSubscription) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::AddOnSubscriptionId,
    ) -> RepoResult<Option<domain::AddOnSubscription>> {
        Ok(None)
    }
    async fn list_active_by_tenant(
        &self,
        _: TenantId,
    ) -> RepoResult<Vec<domain::AddOnSubscription>> {
        Ok(vec![])
    }
    async fn list_by_tenant_and_type(
        &self,
        _: TenantId,
        _: domain::AddOnType,
    ) -> RepoResult<Vec<domain::AddOnSubscription>> {
        Ok(vec![])
    }
}
#[derive(Clone, Default)]
struct NoopSmsProviderPricing;

#[derive(Clone, Default)]
struct NoopHelpCategoryRepository;

#[async_trait]
impl HelpCategoryRepository for NoopHelpCategoryRepository {
    async fn create(&self, _: &domain::entities::HelpCategory) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::shared::ids::HelpCategoryId,
    ) -> RepoResult<Option<domain::entities::HelpCategory>> {
        Ok(None)
    }
    async fn find_by_slug(&self, _: &str) -> RepoResult<Option<domain::entities::HelpCategory>> {
        Ok(None)
    }
    async fn list_all(&self) -> RepoResult<Vec<domain::entities::HelpCategory>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopHelpArticleRepository;

#[async_trait]
impl HelpArticleRepository for NoopHelpArticleRepository {
    async fn create(&self, _: &domain::entities::HelpArticle) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_slug(&self, _: &str) -> RepoResult<Option<domain::entities::HelpArticle>> {
        Ok(None)
    }
    async fn list_by_category(
        &self,
        _: domain::shared::ids::HelpCategoryId,
        _: bool,
    ) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
    async fn search(&self, _: &str) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSupportTicketRepository;

#[async_trait]
impl SupportTicketRepository for NoopSupportTicketRepository {
    async fn create_ticket(&self, _: &domain::entities::SupportTicket) -> RepoResult<()> {
        Ok(())
    }
    async fn find_ticket(
        &self,
        _: domain::shared::ids::SupportTicketId,
    ) -> RepoResult<Option<domain::entities::SupportTicket>> {
        Ok(None)
    }
    async fn list_user_tickets(
        &self,
        _: domain::shared::ids::TenantId,
        _: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<domain::entities::SupportTicket>> {
        Ok(vec![])
    }
    async fn list_tenant_tickets(
        &self,
        _: domain::shared::ids::TenantId,
    ) -> RepoResult<Vec<domain::entities::SupportTicket>> {
        Ok(vec![])
    }
    async fn create_message(&self, _: &domain::entities::SupportMessage) -> RepoResult<()> {
        Ok(())
    }
    async fn list_messages(
        &self,
        _: domain::shared::ids::SupportTicketId,
    ) -> RepoResult<Vec<domain::entities::SupportMessage>> {
        Ok(vec![])
    }
    async fn update_ticket_status(
        &self,
        _: domain::shared::ids::SupportTicketId,
        _: domain::enums::SupportTicketStatus,
    ) -> RepoResult<()> {
        Ok(())
    }
}
#[async_trait]
impl SmsProviderPricing for NoopSmsProviderPricing {
    async fn outbound_sms_unit_price(
        &self,
        _: &str,
    ) -> Result<(rust_decimal::Decimal, String), Box<dyn std::error::Error + Send + Sync>> {
        Err(std::io::Error::other("SMS provider pricing is not configured").into())
    }
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new<M, C, D, I, N, S, A, P, AL>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        id_generator: UuidIdGenerator,
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: AL,
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
        AL: AuditLogEntryRepository + 'static,
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
            secret_cipher,
            id_generator,
            heartbeat_rate_limiter: new_heartbeat_rate_limiter(),
            audit_log_entry_repository: Arc::new(audit_log_entry_repository),
            api_key_repository: Arc::new(NoopApiKeyRepository),
            password_hasher: Arc::new(Argon2PasswordHasher),
            subscription_repository: Arc::new(NoopSubscriptionRepository),
            payment_provider: Arc::new(NoopPaymentProvider),
            checkout_session_repository: Arc::new(NoopCheckoutSessionRepository),
            billing_webhook_secret: None,
            billing_webhook_idempotency_repository: Arc::new(
                NoopBillingWebhookIdempotencyRepository,
            ),
            billing_audit_log_repository: Arc::new(NoopBillingAuditLogRepository),
            pricing_config_repository: Arc::new(NoopPricingConfigRepository),
            agent_lease_repository: None,
            health_check_port: Arc::new(NoopHealthCheckPort),
            dashboard_repository: Arc::new(NoopDashboardRepository),
            subscription_usage_period_repository: Arc::new(NoopSubscriptionUsagePeriodRepository),
            subscription_usage_transaction: Arc::new(NoopSubscriptionUsageTransaction),
            add_on_subscription_repository: Arc::new(NoopAddOnSubscriptionRepository),
            sms_provider_pricing: Arc::new(NoopSmsProviderPricing),
            help_category_repository: Arc::new(NoopHelpCategoryRepository),
            help_article_repository: Arc::new(NoopHelpArticleRepository),
            support_ticket_repository: Arc::new(NoopSupportTicketRepository),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_feature_repos<M, C, D, I, N, S, A, P, MW, EP, SP, CDV, AL>(
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
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: AL,
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
        AL: AuditLogEntryRepository + 'static,
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
            secret_cipher,
            id_generator,
            heartbeat_rate_limiter: new_heartbeat_rate_limiter(),
            audit_log_entry_repository: Arc::new(audit_log_entry_repository),
            api_key_repository: Arc::new(NoopApiKeyRepository),
            password_hasher: Arc::new(Argon2PasswordHasher),
            subscription_repository: Arc::new(NoopSubscriptionRepository),
            payment_provider: Arc::new(NoopPaymentProvider),
            checkout_session_repository: Arc::new(NoopCheckoutSessionRepository),
            billing_webhook_secret: None,
            billing_webhook_idempotency_repository: Arc::new(
                NoopBillingWebhookIdempotencyRepository,
            ),
            billing_audit_log_repository: Arc::new(NoopBillingAuditLogRepository),
            pricing_config_repository: Arc::new(NoopPricingConfigRepository),
            agent_lease_repository: None,
            health_check_port: Arc::new(NoopHealthCheckPort),
            dashboard_repository: Arc::new(NoopDashboardRepository),
            subscription_usage_period_repository: Arc::new(NoopSubscriptionUsagePeriodRepository),
            subscription_usage_transaction: Arc::new(NoopSubscriptionUsageTransaction),
            add_on_subscription_repository: Arc::new(NoopAddOnSubscriptionRepository),
            sms_provider_pricing: Arc::new(NoopSmsProviderPricing),
            help_category_repository: Arc::new(NoopHelpCategoryRepository),
            help_article_repository: Arc::new(NoopHelpArticleRepository),
            support_ticket_repository: Arc::new(NoopSupportTicketRepository),
        }
    }

    pub fn with_api_key_repository<R>(mut self, api_key_repository: R) -> Self
    where
        R: ApiKeyRepository + 'static,
    {
        self.api_key_repository = Arc::new(api_key_repository);
        self
    }

    /// Substitutes a real `StatusPageRepository` in place of the
    /// `NoopStatusPageRepository` default. Mirrors `with_api_key_repository`.
    pub fn with_status_page_repository<R>(mut self, status_page_repository: R) -> Self
    where
        R: StatusPageRepository + 'static,
    {
        self.status_page_repository = Arc::new(status_page_repository);
        self
    }

    pub fn with_subscription_repository<R>(mut self, subscription_repository: R) -> Self
    where
        R: SubscriptionRepository + 'static,
    {
        self.subscription_repository = Arc::new(subscription_repository);
        self
    }

    pub fn with_payment_provider<P>(mut self, payment_provider: P) -> Self
    where
        P: PaymentProvider + 'static,
    {
        self.payment_provider = Arc::new(payment_provider);
        self
    }

    pub fn with_checkout_session_repository<R>(mut self, checkout_session_repository: R) -> Self
    where
        R: CheckoutSessionRepository + 'static,
    {
        self.checkout_session_repository = Arc::new(checkout_session_repository);
        self
    }

    pub fn with_billing_webhook_secret<S: Into<String>>(mut self, secret: S) -> Self {
        self.billing_webhook_secret = Some(Arc::from(secret.into()));
        self
    }

    pub fn with_billing_webhook_idempotency_repository<R>(mut self, repo: R) -> Self
    where
        R: BillingWebhookIdempotencyRepository + 'static,
    {
        self.billing_webhook_idempotency_repository = Arc::new(repo);
        self
    }

    pub fn with_billing_audit_log_repository<R>(mut self, repo: R) -> Self
    where
        R: BillingAuditLogRepository + 'static,
    {
        self.billing_audit_log_repository = Arc::new(repo);
        self
    }

    pub fn with_agent_lease_repository<R>(mut self, agent_lease_repository: R) -> Self
    where
        R: AgentLeaseRepository + 'static,
    {
        self.agent_lease_repository = Some(Arc::new(agent_lease_repository));
        self
    }

    /// B34: substitutes a real `HealthCheckPort` (see
    /// `infra::PostgresHealthCheck`) in place of the always-healthy
    /// `NoopHealthCheckPort` default, backing `GET /ready`.
    pub fn with_health_check_port<H>(mut self, health_check_port: H) -> Self
    where
        H: HealthCheckPort + 'static,
    {
        self.health_check_port = Arc::new(health_check_port);
        self
    }

    /// H6 — Dashboard Overview:
    /// Substitutes a real `DashboardRepository` (e.g. `PostgresDashboardRepository`)
    /// in place of the no-op default.
    pub fn with_pricing_config_repository<R>(mut self, pricing_config_repository: R) -> Self
    where
        R: PricingConfigRepository + 'static,
    {
        self.pricing_config_repository = Arc::new(pricing_config_repository);
        self
    }

    pub fn with_dashboard_repository<R>(mut self, dashboard_repository: R) -> Self
    where
        R: DashboardRepository + 'static,
    {
        self.dashboard_repository = Arc::new(dashboard_repository);
        self
    }

    /// H12 — Proration Service.
    pub fn with_subscription_usage_period_repository<R>(mut self, repo: R) -> Self
    where
        R: SubscriptionUsagePeriodRepository + 'static,
    {
        self.subscription_usage_period_repository = Arc::new(repo);
        self
    }

    /// H12 — Proration Service.
    pub fn with_add_on_subscription_repository<R>(mut self, repo: R) -> Self
    where
        R: AddOnSubscriptionRepository + 'static,
    {
        self.add_on_subscription_repository = Arc::new(repo);
        self
    }

    pub fn with_sms_provider_pricing<P>(mut self, provider: P) -> Self
    where
        P: SmsProviderPricing + 'static,
    {
        self.sms_provider_pricing = Arc::new(provider);
        self
    }

    pub fn with_subscription_usage_transaction<T>(mut self, transaction: T) -> Self
    where
        T: SubscriptionUsageTransaction + 'static,
    {
        self.subscription_usage_transaction = Arc::new(transaction);
        self
    }

    pub fn with_help_category_repository<R>(mut self, repo: R) -> Self
    where
        R: HelpCategoryRepository + 'static,
    {
        self.help_category_repository = Arc::new(repo);
        self
    }

    pub fn with_help_article_repository<R>(mut self, repo: R) -> Self
    where
        R: HelpArticleRepository + 'static,
    {
        self.help_article_repository = Arc::new(repo);
        self
    }

    pub fn with_support_ticket_repository<R>(mut self, repo: R) -> Self
    where
        R: SupportTicketRepository + 'static,
    {
        self.support_ticket_repository = Arc::new(repo);
        self
    }
}
