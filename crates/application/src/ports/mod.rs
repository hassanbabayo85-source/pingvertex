pub mod agent_registry;
pub mod api_flow_repository;
pub mod check_repository;
pub mod diagnostic_report_repository;
pub mod http_client;
pub mod id_generator;
pub mod incident_repository;
pub mod monitor_interval_policy_repository;
pub mod monitor_repository;
pub mod notification_channel_repository;
pub mod notification_outbox_repository;
pub mod notification_outbox_sender;
pub mod notification_service;
pub mod password_hasher;
pub mod proxy_check_job_repository;
pub mod proxy_credential_cipher;
pub mod recurring_proxy_monitor_repository;
pub mod secret_cipher;
pub mod subscriber_repository;
pub mod token_service;
pub mod user_repository;

// Sabbin ports:
pub mod api_key_repository;
pub mod audit_log_entry_repository;
pub mod backup_code_repository;
pub mod custom_domain_verifier;
pub mod escalation_policy_repository;
pub mod maintenance_window_repository;
pub mod pending_auth_cache;
pub mod rate_limiter;
pub mod registration_transaction;
pub mod status_page_cache;
pub mod status_page_repository;
pub mod tenant_membership_repository;
pub mod tenant_repository;

// Email verification:
pub mod email_verification_mailer;
pub mod email_verification_token_repository;

// OAuth — Continue with Google / Apple:
pub mod id_token_verifier;

// Tenant invitations:
pub mod tenant_invitation_repository;

// Public Endpoint Security & Efficiency:
pub mod health_check;

// Dashboard Overview:
pub mod dashboard_repository;

// Help Center & Support:
pub mod help_article_repository;
pub mod help_category_repository;
pub mod support_ticket_repository;

pub use api_flow_repository::ApiFlowRepository;
pub use check_repository::CheckRepository;
pub use diagnostic_report_repository::DiagnosticReportRepository;
pub use http_client::*;
pub use id_generator::*;
pub use incident_repository::IncidentRepository;
// Atomic Quota Enforcement: re-export QuotaCheckOutcome alongside
// MonitorRepository so callers can `use application::ports::{MonitorRepository, QuotaCheckOutcome};`
pub use monitor_interval_policy_repository::MonitorIntervalPolicyRepository;
pub use monitor_repository::{
    BulkImportExistingMonitor, BulkImportMonitorSnapshot, MonitorRepository, QuotaCheckOutcome,
};
pub use notification_channel_repository::NotificationChannelRepository;
pub use notification_outbox_repository::NotificationOutboxRepository;
pub use notification_outbox_sender::NotificationOutboxSender;
pub use notification_service::{NotificationService, NotifyResult};
pub use password_hasher::PasswordHasher;
pub use proxy_check_job_repository::{ProxyCheckJobRepository, ProxyResultStatusFilter};
pub use proxy_credential_cipher::{
    EncryptedProxyCredentials, ProxyCredentialCipher, ProxyCredentialCipherError,
};
pub use recurring_proxy_monitor_repository::{
    ClaimedRecurringProxyMonitor, RecurringProxyMonitorRepository,
};
pub use secret_cipher::{SecretCipher, SecretCipherError};
pub use subscriber_repository::SubscriberRepository;
pub use token_service::{TokenService, VerifiedToken};
pub use user_repository::UserRepository;

// Sabbin re-exports:
pub use api_key_repository::ApiKeyRepository;
pub use audit_log_entry_repository::{
    AuditLogCursor, AuditLogEntryRepository, AuditLogFilter, AuditLogPage,
};
pub use backup_code_repository::BackupCodeRepository;
pub use custom_domain_verifier::{CustomDomainVerifier, VerifierResult};
pub use escalation_policy_repository::EscalationPolicyRepository;
pub use maintenance_window_repository::MaintenanceWindowRepository;
pub use pending_auth_cache::{PendingAuthCache, PendingAuthCacheResult};
pub use rate_limiter::{RateLimiter, RateLimiterResult};
pub use registration_transaction::{RegistrationTransaction, RegistrationTransactionResult};
// Public Endpoint Security & Efficiency: PUBLIC_STATUS_LIST_CACHE_KEY
// is the shared cache key for the public status *list* endpoint, used by
// both the API (get_public_status) and the worker (IncidentManager).
pub use status_page_cache::{CacheResult, PUBLIC_STATUS_LIST_CACHE_KEY, StatusPageCache};
pub use status_page_repository::StatusPageRepository;
pub use tenant_membership_repository::{LastOwnerGuardOutcome, TenantMembershipRepository};
pub use tenant_repository::TenantRepository;

pub mod logo_storage;
pub use logo_storage::{LogoStorage, StorageResult};

// Email verification re-exports:
pub use email_verification_mailer::{EmailVerificationMailer, MailerResult};
pub use email_verification_token_repository::EmailVerificationTokenRepository;

// OAuth re-exports:
pub use id_token_verifier::{IdTokenVerifier, OidcVerifierResult, VerifiedIdentity};
pub mod oauth_identity_repository;
pub use oauth_identity_repository::{OAuthIdentityRepoResult, OAuthIdentityRepository};

// Password reset:
pub mod password_reset_mailer;
pub mod reset_password_token_repository;
pub use password_reset_mailer::{PasswordResetMailer, PasswordResetMailerResult};
pub use reset_password_token_repository::ResetPasswordTokenRepository;

// Tenant invitation re-export:
pub use tenant_invitation_repository::TenantInvitationRepository;

// Plan / Subscription / Entitlement model:
pub mod entitlement_resolver;
pub use entitlement_resolver::EntitlementResolver;

// Billing Provider Abstraction & Checkout:

// Billing Webhooks:

// Durable Scheduler:
pub mod scheduling_repository;
pub use scheduling_repository::{ClaimedMonitor, ClaimedOneTimeExecution, SchedulingRepository};

// Agent Execution & mTLS:
pub mod agent_lease_repository;
pub use agent_lease_repository::{
    AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome,
};

// History Retention Enforcement:
pub mod history_retention_repository;
pub use history_retention_repository::HistoryRetentionRepository;

// Public Endpoint Security & Efficiency:
pub use health_check::{HealthCheckPort, HealthCheckResult};

// Dashboard Overview:
pub use dashboard_repository::DashboardRepository;
pub mod pricing_config_repository;
pub use pricing_config_repository::PricingConfigRepository;

// Proration Service:


pub mod sms_provider_pricing;
pub use sms_provider_pricing::{SmsPricingResult, SmsProviderPricing};

// Help Center & Support:
pub use help_article_repository::HelpArticleRepository;
pub use help_category_repository::HelpCategoryRepository;
pub use support_ticket_repository::SupportTicketRepository;

// RUM (Real User Monitoring):
pub mod geo_ip_lookup;
pub mod rum_analytics_repository;
pub mod rum_event_repository;
pub mod rum_retention_repository;
pub use rum_retention_repository::{RumRetentionOutcome, RumRetentionRepository};
pub mod rum_site_repository;
pub use geo_ip_lookup::GeoIpLookup;
pub use rum_analytics_repository::RumAnalyticsRepository;
pub use rum_event_repository::{RumEventRepository, UpsertedRumSession};
pub use rum_site_repository::RumSiteRepository;

pub mod tenant_alert_allowance_repository;
pub mod tenant_retention_settings_repository;
pub use tenant_alert_allowance_repository::{
    RepoResult as AlertAllowanceRepoResult, TenantAlertAllowanceRepository,
};
pub use tenant_retention_settings_repository::{
    RepoResult as TenantRetentionRepoResult, TenantRetentionSettingsRepository,
};

pub mod usage_event_repository;
pub use usage_event_repository::{UsageDailyPoint, UsageOperationPoint, UsageReport};
pub use usage_event_repository::{UsageEventRepoResult, UsageEventRepository};


// PAYG durable CSV bulk import.
pub mod bulk_monitor_action_repository;
pub mod bulk_monitor_import_repository;
pub use bulk_monitor_import_repository::{
    BulkMonitorImportRepoResult, BulkMonitorImportRepository,
};

pub use bulk_monitor_action_repository::{
    BulkMonitorActionJob, BulkMonitorActionRepoResult, BulkMonitorActionRepository,
};

pub use agent_registry::{AgentRegistry, AgentRegistryResult};

// Task 6 — server-side plan prices:

// Task 6 — subscription billing records:
