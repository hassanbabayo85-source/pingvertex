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
}
