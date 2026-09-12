use thiserror::Error;

/// Common application errors shared across crates.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Resource not found")]
    NotFound,

    #[error("Invalid input: {0}")]
    Validation(String),

    #[error("Internal error")]
    Internal,
}
