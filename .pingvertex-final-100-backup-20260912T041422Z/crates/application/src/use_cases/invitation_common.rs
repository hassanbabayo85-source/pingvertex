use std::error::Error;
use std::fmt;

/// Shared error type for tenant invitation use cases.
#[derive(Debug)]
pub enum InvitationError {
    Forbidden,
    NotFound,
    Conflict,
    InvalidToken,
    InvalidOperation(String),
    Internal(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for InvitationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvitationError::Forbidden => write!(f, "forbidden"),
            InvitationError::NotFound => write!(f, "not found"),
            InvitationError::Conflict => write!(f, "conflict"),
            InvitationError::InvalidToken => write!(f, "invalid or expired invitation"),
            InvitationError::InvalidOperation(msg) => write!(f, "invalid operation: {msg}"),
            InvitationError::Internal(err) => write!(f, "internal error: {err}"),
        }
    }
}

impl Error for InvitationError {}
