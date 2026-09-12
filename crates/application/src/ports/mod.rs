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
pub mod financial_audit_log_repository;
pub mod maintenance_window_repository;
pub mod pending_auth_cache;
pub mod rate_limiter;
pub mod registration_transaction;
pub mod status_page_cache;
pub mod status_page_repository;
pub mod tenant_membership_repository;
pub mod tenant_repository;

// B5 — Email verification:
pub mod email_verification_mailer;
pub mod email_verification_token_repository;

// OAuth — Continue with Google / Apple:
pub mod id_token_verifier;

// B9 — Tenant invitations:
pub mod tenant_invitation_repository;

// B34 — Public Endpoint Security & Efficiency:
pub mod health_check;

// H6 — Dashboard Overview:
pub mod dashboard_repository;

// H14 — Help Center & Support:
pub mod help_article_repository;
pub mod help_category_repository;
pub mod support_ticket_repository;

pub use api_flow_repository::ApiFlowRepository;
pub use check_repository::CheckRepository;
pub use diagnostic_report_repository::DiagnosticReportRepository;
pub use http_client::*;
pub use id_generator::*;
pub use incident_repository::IncidentRepository;
// B13 — Atomic Quota Enforcement: re-export QuotaCheckOutcome alongside
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
pub use financial_audit_log_repository::{
    FinancialAuditLogCursor, FinancialAuditLogFilter, FinancialAuditLogPage,
    FinancialAuditLogRepository, FinancialAuditRepoResult,
};
pub use maintenance_window_repository::MaintenanceWindowRepository;
pub use pending_auth_cache::{PendingAuthCache, PendingAuthCacheResult};
pub use rate_limiter::{RateLimiter, RateLimiterResult};
pub use registration_transaction::{RegistrationTransaction, RegistrationTransactionResult};
// B34 — Public Endpoint Security & Efficiency: PUBLIC_STATUS_LIST_CACHE_KEY
// is the shared cache key for the public status *list* endpoint, used by
// both the API (get_public_status) and the worker (IncidentManager).
pub use status_page_cache::{CacheResult, PUBLIC_STATUS_LIST_CACHE_KEY, StatusPageCache};
pub use status_page_repository::StatusPageRepository;
pub use tenant_membership_repository::{LastOwnerGuardOutcome, TenantMembershipRepository};
pub use tenant_repository::TenantRepository;

pub mod logo_storage;
pub use logo_storage::{LogoStorage, StorageResult};

// B5 — Email verification re-exports:
pub use email_verification_mailer::{EmailVerificationMailer, MailerResult};
pub use email_verification_token_repository::EmailVerificationTokenRepository;

// OAuth re-exports:
pub use id_token_verifier::{IdTokenVerifier, OidcVerifierResult, VerifiedIdentity};
pub mod oauth_identity_repository;
pub use oauth_identity_repository::{OAuthIdentityRepoResult, OAuthIdentityRepository};

// B6 — Password reset:
pub mod password_reset_mailer;
pub mod reset_password_token_repository;
pub use password_reset_mailer::{PasswordResetMailer, PasswordResetMailerResult};
pub use reset_password_token_repository::ResetPasswordTokenRepository;

// B9 — Tenant invitation re-export:
pub use tenant_invitation_repository::TenantInvitationRepository;

// B11 — Plan / Subscription / Entitlement model:
pub mod subscription_repository;
pub use subscription_repository::SubscriptionRepository;

// B14 — Billing Provider Abstraction & Checkout:
pub mod checkout_session_repository;
pub mod payment_provider;
pub use checkout_session_repository::CheckoutSessionRepository;
pub use payment_provider::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
};
pub use payment_provider::{RefundDetails, RefundRequest, WalletTopUpRequest};

// B15 — Billing Webhooks:
pub mod billing_audit_log_repository;
pub mod billing_webhook_idempotency_repository;
pub use billing_audit_log_repository::BillingAuditLogRepository;
pub use billing_webhook_idempotency_repository::BillingWebhookIdempotencyRepository;

// B17 — Durable Scheduler:
pub mod scheduling_repository;
pub use scheduling_repository::{ClaimedMonitor, ClaimedOneTimeCheck, SchedulingRepository};

// B20 — Agent Execution & mTLS:
pub mod agent_lease_repository;
pub use agent_lease_repository::{
    AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome,
};

// B22 — History Retention Enforcement:
pub mod history_retention_repository;
pub use history_retention_repository::HistoryRetentionRepository;

// B34 — Public Endpoint Security & Efficiency:
pub use health_check::{HealthCheckPort, HealthCheckResult};

// H6 — Dashboard Overview:
pub use dashboard_repository::DashboardRepository;
pub mod pricing_config_repository;
pub use pricing_config_repository::PricingConfigRepository;

// H12 — Proration Service:
pub mod subscription_usage_period_repository;
pub mod subscription_usage_transaction;
pub use subscription_usage_period_repository::SubscriptionUsagePeriodRepository;
pub use subscription_usage_transaction::{
    SubscriptionUsageTransaction, SubscriptionUsageTransactionResult,
};

pub mod add_on_subscription_repository;
pub use add_on_subscription_repository::AddOnSubscriptionRepository;

pub mod sms_provider_pricing;
pub use sms_provider_pricing::{SmsPricingResult, SmsProviderPricing};

// H14 — Help Center & Support:
pub use help_article_repository::HelpArticleRepository;
pub use help_category_repository::HelpCategoryRepository;
pub use support_ticket_repository::SupportTicketRepository;

// RUM (Real User Monitoring):
pub mod geo_ip_lookup;
pub mod rum_analytics_repository;
pub mod rum_event_repository;
pub mod rum_site_repository;
pub use geo_ip_lookup::GeoIpLookup;
pub use rum_analytics_repository::RumAnalyticsRepository;
pub use rum_event_repository::RumEventRepository;
pub use rum_site_repository::RumSiteRepository;

pub mod invoice_repository;
pub mod subscription_plan_change_repository;
pub mod wallet_repository;
pub use invoice_repository::{InvoiceRepoResult, InvoiceRepository};
pub use subscription_plan_change_repository::{
    PlanChangeRepoResult, SubscriptionPlanChangeRepository,
};
pub use wallet_repository::{ReservationOutcome, WalletRepoResult, WalletRepository};

pub mod payg_pricing_repository;
pub use payg_pricing_repository::{
    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext, resolve_price,
};
pub mod usage_event_repository;
pub use usage_event_repository::{UsageDailyPoint, UsageOperationPoint, UsageReport};
pub use usage_event_repository::{UsageEventRepoResult, UsageEventRepository};

pub mod payment_repository;
pub use payment_repository::{PaymentRepoResult, PaymentRepository};

// PAYG Part 3 — durable CSV bulk import.
pub mod bulk_monitor_action_repository;
pub mod bulk_monitor_import_repository;
pub use bulk_monitor_import_repository::{
    BulkMonitorImportRepoResult, BulkMonitorImportRepository,
};

pub use bulk_monitor_action_repository::{
    BulkMonitorActionJob, BulkMonitorActionRepoResult, BulkMonitorActionRepository,
};
