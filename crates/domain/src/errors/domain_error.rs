use thiserror::Error;

/// Errors produced by the domain layer.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("monitor name cannot be empty")]
    EmptyMonitorName,

    #[error("monitor name cannot exceed 100 characters")]
    MonitorNameTooLong,

    #[error("url cannot be empty")]
    EmptyUrl,

    #[error("invalid url")]
    InvalidUrl,

    #[error("url must not exceed 2048 characters")]
    UrlTooLong,

    #[error("only http and https urls are supported")]
    UnsupportedUrlScheme,

    #[error("check interval must be at least 30 seconds")]
    IntervalTooSmall,

    #[error("check interval cannot exceed 86400 seconds")]
    IntervalTooLarge,

    #[error("timeout must be at least 1 second")]
    TimeoutTooSmall,

    #[error("timeout cannot exceed 300 seconds")]
    TimeoutTooLarge,

    #[error("expected status must be between 100 and 599")]
    InvalidExpectedStatus,

    #[error("notification channel target cannot be empty")]
    EmptyNotificationTarget,

    #[error("invalid email address for email notification channel")]
    InvalidNotificationEmail,

    #[error("invalid url for webhook notification channel")]
    InvalidNotificationWebhookUrl,

    #[error("extraction name cannot be empty")]
    EmptyExtractionName,

    #[error("extraction name cannot exceed 64 characters")]
    ExtractionNameTooLong,

    #[error("extraction name may only contain alphanumeric characters and underscores")]
    InvalidExtractionNameCharacters,

    #[error("flow step name cannot be empty")]
    EmptyFlowStepName,

    #[error("flow step name cannot exceed 100 characters")]
    FlowStepNameTooLong,

    #[error("an api flow cannot have more than 10 steps")]
    TooManyFlowSteps,

    #[error("url template cannot be empty")]
    EmptyUrlTemplate,

    #[error("url template must not exceed 2048 characters")]
    UrlTemplateTooLong,

    #[error("header value must not exceed 2048 characters")]
    HeaderValueTooLong,

    #[error("flow step body cannot exceed 65536 bytes (64KB)")]
    FlowStepBodyTooLarge,

    #[error("an api flow must have at least one step")]
    EmptyFlowSteps,

    #[error("flow step order must be sequential starting at 1 with no gaps or duplicates")]
    InvalidStepOrder,

    #[error("a flow step cannot have more than 20 headers")]
    TooManyHeaders,

    #[error("proxy host cannot be empty")]
    EmptyProxyHost,

    #[error("proxy host must not exceed 253 characters")]
    ProxyHostTooLong,

    #[error("invalid proxy host")]
    InvalidProxyHost,

    #[error("private, loopback, or link-local addresses are not allowed as a proxy host")]
    ProxyHostIsPrivateOrReserved,

    #[error("proxy port is not allowed; only known proxy ports are permitted")]
    ProxyPortNotAllowed,

    #[error("a proxy check job cannot have more than 500 proxy entries")]
    TooManyProxyEntries,

    #[error("proxy entries cannot be empty")]
    EmptyProxyEntries,

    #[error("proxy credentials username and password cannot both be empty")]
    EmptyProxyCredentials,

    #[error("proxy credential fields must not exceed 256 characters")]
    ProxyCredentialFieldTooLong,

    #[error("unsupported region code")]
    UnsupportedRegionCode,

    // Sabbin variants:
    #[error("invalid escalation steps: {0}")]
    InvalidEscalationSteps(String),

    #[error("invalid maintenance reason: {0}")]
    InvalidMaintenanceReason(String),

    #[error("invalid maintenance window: {0}")]
    InvalidMaintenanceWindow(String),

    #[error("subdomain cannot be empty")]
    EmptySubdomain,

    #[error("subdomain cannot exceed 63 characters")]
    SubdomainTooLong,

    #[error(
        "subdomain may only contain lowercase letters, digits, and hyphens, and cannot start or end with a hyphen"
    )]
    InvalidSubdomainCharacters,

    #[error("this subdomain is reserved and cannot be used")]
    ReservedSubdomain,

    #[error("hex color must match the format #RRGGBB")]
    InvalidHexColor,

    #[error("custom domain cannot be empty")]
    EmptyCustomDomain,

    #[error("custom domain must not exceed 253 characters")]
    CustomDomainTooLong,

    #[error("invalid custom domain")]
    InvalidCustomDomain,

    #[error("status page name cannot be empty")]
    EmptyStatusPageName,

    #[error("status page name cannot exceed 100 characters")]
    StatusPageNameTooLong,

    #[error("a status page cannot have more than 50 monitors")]
    TooManyStatusPageMonitors,

    #[error("logo url must not exceed 2048 characters")]
    LogoUrlTooLong,

    #[error("invalid logo url")]
    InvalidLogoUrl,

    #[error("logo image cannot be empty")]
    EmptyLogoImage,

    #[error("logo image must not exceed 500 KB")]
    LogoImageTooLarge,

    #[error("logo image must be a PNG, JPEG, GIF, or WebP file")]
    UnsupportedLogoImageType,

    #[error("invalid slack webhook URL; must be https://hooks.slack.com/...")]
    InvalidSlackWebhookUrl,

    #[error("invalid discord webhook URL; must be https://discord.com/api/webhooks/...")]
    InvalidDiscordWebhookUrl,

    #[error("invalid telegram bot token")]
    InvalidTelegramBotToken,

    #[error("invalid phone number; must be E.164 format, e.g. +2348012345678")]
    InvalidPhoneNumber,

    #[error("invalid telegram chat id; must be a numeric id, e.g. 123456789 or -1001234567890")]
    InvalidTelegramChatId,

    #[error("invalid monitor kind fields: {0}")]
    InvalidMonitorKindFields(String),

    #[error("invalid heartbeat token")]
    InvalidHeartbeatToken,

    #[error("tenant name cannot be empty")]
    EmptyTenantName,

    #[error("tenant name cannot exceed 100 characters")]
    TenantNameTooLong,
}
