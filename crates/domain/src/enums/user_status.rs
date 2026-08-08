use std::fmt;

/// Lifecycle state of a user account.
///
/// Authentication and authorization decisions can
/// depend on the current status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UserStatus {
    /// Registered but email not yet verified.
    #[default]
    PendingVerification,

    /// Fully active account.
    Active,

    /// Account temporarily disabled.
    Suspended,

    /// Soft-deleted account.
    Deleted,
}

impl fmt::Display for UserStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserStatus::PendingVerification => {
                write!(f, "pending_verification")
            }
            UserStatus::Active => write!(f, "active"),
            UserStatus::Suspended => write!(f, "suspended"),
            UserStatus::Deleted => write!(f, "deleted"),
        }
    }
}
