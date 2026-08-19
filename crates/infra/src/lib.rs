pub mod config;
pub mod db;
pub mod repositories;
pub mod s3_config;
pub mod services;

pub use config::{NotificationConfigError, SmtpConfig};
pub use db::create_pool;
pub use repositories::postgres_diagnostic_report_repository::PostgresDiagnosticReportRepository;
pub use repositories::*;
pub use s3_config::{S3Config, S3ConfigError};
pub use services::{
    AesGcmProxyCredentialCipher, Argon2PasswordHasher, Claims, DisabledEmailNotificationService,
    DiscordNotificationService, HickoryCustomDomainVerifier, InMemoryLogoStorage,
    JsonWebTokenService, NoopNotificationService, NotificationDispatcher,
    ProxyCredentialEncryptionKeyError, RedisStatusPageCache, S3LogoStorage,
    SlackNotificationService, SmsNotificationService, SmtpEmailNotificationService,
    TelegramNotificationService, UuidIdGenerator, WebhookNotificationService,
};
