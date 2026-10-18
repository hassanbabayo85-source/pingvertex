use std::fmt;

/// Represents a hashed password.
///
/// NOTE:
/// This type NEVER stores or accepts a plain-text password.
/// Hashing is performed by the Application layer through the
/// PasswordHasher port and implemented in Infrastructure.
#[derive(Clone, PartialEq, Eq)]
pub struct PasswordHash(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordHashError {
    Empty,
}

impl fmt::Display for PasswordHashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasswordHashError::Empty => {
                write!(f, "password hash cannot be empty")
            }
        }
    }
}

impl std::error::Error for PasswordHashError {}

impl PasswordHash {
    /// Creates a PasswordHash from an already-generated hash.
    ///
    /// This MUST only be called by infrastructure after hashing.
    pub fn new(hash: impl Into<String>) -> Result<Self, PasswordHashError> {
        let hash = hash.into();

        if hash.trim().is_empty() {
            return Err(PasswordHashError::Empty);
        }

        Ok(Self(hash))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for PasswordHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for PasswordHash {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Manual Debug that redacts the inner hash. `Display` is
/// intentionally left unchanged: it is used by callers that genuinely
/// need the hash string (persisting it, comparing it, etc).
impl std::fmt::Debug for PasswordHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PasswordHash").field(&"[REDACTED]").finish()
    }
}
