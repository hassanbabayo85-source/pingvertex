use std::error::Error;
use std::fmt;

/// Shared error type for tenant membership use cases.
#[derive(Debug)]
pub enum MembershipError {
    Forbidden,
    NotFound,
    Conflict,
    InvalidOperation(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for MembershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembershipError::Forbidden => write!(f, "forbidden"),
            MembershipError::NotFound => write!(f, "not found"),
            MembershipError::Conflict => write!(f, "conflict"),
            MembershipError::InvalidOperation(msg) => write!(f, "invalid operation: {msg}"),
            MembershipError::Other(err) => write!(f, "internal error: {err}"),
        }
    }
}

impl Error for MembershipError {}
