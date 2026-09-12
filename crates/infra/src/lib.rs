pub mod config;
pub mod db;
pub mod repositories;
pub mod s3_config;
pub mod services;
pub mod twilio_config;
pub mod twilio_pricing;
// B34 — Public Endpoint Security & Efficiency: readiness-endpoint DB check.
pub mod postgres_health_check;
pub use config::{NotificationConfigError, SmtpConfig};
pub use db::create_pool;
pub use postgres_health_check::PostgresHealthCheck;
pub use repositories::postgres_diagnostic_report_repository::PostgresDiagnosticReportRepository;
pub use repositories::postgres_pricing_config_repository::PostgresPricingConfigRepository;
pub use repositories::*;
pub use s3_config::{S3Config, S3ConfigError};
pub use services::{
    AesGcmProxyCredentialCipher, Argon2PasswordHasher, Claims, DisabledEmailNotificationService,
    DisabledEmailVerificationMailer, DisabledIdTokenVerifier, DisabledPasswordResetMailer,
    DiscordNotificationService, HickoryCustomDomainVerifier, InMemoryLogoStorage,
    JsonWebTokenService, NoopNotificationService, NotificationDispatcher, OidcIdTokenVerifier,
    ProxyCredentialEncryptionKeyError, RedisPendingAuthCache, RedisRateLimiter,
    RedisStatusPageCache, S3LogoStorage, SlackNotificationService, SmsNotificationService,
    SmtpEmailNotificationService, SmtpEmailVerificationMailer, SmtpPasswordResetMailer,
    TelegramNotificationService, UuidIdGenerator, WebhookNotificationService,
};
pub use twilio_config::{TwilioConfig, TwilioConfigError};
pub use twilio_pricing::TwilioPricingService;

// B14 — Billing Provider Abstraction & Checkout:
pub mod billing_config;
pub use billing_config::{BillingConfig, BillingConfigError};
pub use services::DisabledBillingProvider;
pub use services::{FakeBillingProvider, PaystackBillingProvider};

pub mod test_support;
