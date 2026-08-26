pub mod api_flow_repository;
pub mod check_repository;
pub mod diagnostic_report_repository;
pub mod http_client;
pub mod id_generator;
pub mod incident_repository;
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

// B9 — Tenant invitations:
pub mod tenant_invitation_repository;

// B34 — Public Endpoint Security & Efficiency:
pub mod health_check;

// H6 — Dashboard Overview:
pub mod dashboard_repository;

pub use api_flow_repository::ApiFlowRepository;
pub use check_repository::CheckRepository;
pub use diagnostic_report_repository::DiagnosticReportRepository;
pub use http_client::*;
pub use id_generator::*;
pub use incident_repository::IncidentRepository;
// B13 — Atomic Quota Enforcement: re-export QuotaCheckOutcome alongside
// MonitorRepository so callers can `use application::ports::{MonitorRepository, QuotaCheckOutcome};`
pub use monitor_repository::{MonitorRepository, QuotaCheckOutcome};
pub use notification_channel_repository::NotificationChannelRepository;
pub use notification_outbox_repository::NotificationOutboxRepository;
pub use notification_outbox_sender::NotificationOutboxSender;
pub use notification_service::{NotificationService, NotifyResult};
pub use password_hasher::PasswordHasher;
pub use proxy_check_job_repository::ProxyCheckJobRepository;
pub use proxy_credential_cipher::{
    EncryptedProxyCredentials, ProxyCredentialCipher, ProxyCredentialCipherError,
};
pub use secret_cipher::{SecretCipher, SecretCipherError};
pub use subscriber_repository::SubscriberRepository;
pub use token_service::{TokenService, VerifiedToken};
pub use user_repository::UserRepository;

// Sabbin re-exports:
pub use api_key_repository::ApiKeyRepository;
pub use audit_log_entry_repository::AuditLogEntryRepository;
pub use backup_code_repository::BackupCodeRepository;
pub use custom_domain_verifier::{CustomDomainVerifier, VerifierResult};
pub use escalation_policy_repository::EscalationPolicyRepository;
pub use maintenance_window_repository::MaintenanceWindowRepository;
pub use pending_auth_cache::{PendingAuthCache, PendingAuthCacheResult};
pub use rate_limiter::{RateLimiter, RateLimiterResult};
pub use registration_transaction::{RegistrationTransaction, RegistrationTransactionResult};
// B34 — Public Endpoint Security & Efficiency: PUBLIC_STATUS_LIST_CACHE_KEY
// is the shared cache key for the public status *list* endpoint, used by
// both the API (get_public_status) and the worker (IncidentManager).
pub use status_page_cache::{CacheResult, PUBLIC_STATUS_LIST_CACHE_KEY, StatusPageCache};
pub use status_page_repository::StatusPageRepository;
pub use tenant_membership_repository::TenantMembershipRepository;
pub use tenant_repository::TenantRepository;

pub mod logo_storage;
pub use logo_storage::{LogoStorage, StorageResult};

// B5 — Email verification re-exports:
pub use email_verification_mailer::{EmailVerificationMailer, MailerResult};
pub use email_verification_token_repository::EmailVerificationTokenRepository;

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

// B15 — Billing Webhooks:
pub mod billing_audit_log_repository;
pub mod billing_webhook_idempotency_repository;
pub use billing_audit_log_repository::BillingAuditLogRepository;
pub use billing_webhook_idempotency_repository::BillingWebhookIdempotencyRepository;

// B17 — Durable Scheduler:
pub mod scheduling_repository;
pub use scheduling_repository::SchedulingRepository;

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
