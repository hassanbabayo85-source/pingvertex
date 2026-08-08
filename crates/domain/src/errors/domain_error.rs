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
}
