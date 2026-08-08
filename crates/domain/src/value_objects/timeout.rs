use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Validated monitor timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeout(u32);

impl Timeout {
    pub const MIN: u32 = 1;
    pub const MAX: u32 = 300;

    /// Creates a validated timeout.
    pub fn new(seconds: u32) -> Result<Self, DomainError> {
        if seconds < Self::MIN {
            return Err(DomainError::TimeoutTooSmall);
        }

        if seconds > Self::MAX {
            return Err(DomainError::TimeoutTooLarge);
        }

        Ok(Self(seconds))
    }

    /// Returns timeout in seconds.
    pub fn as_seconds(self) -> u32 {
        self.0
    }
}

impl From<Timeout> for u32 {
    fn from(value: Timeout) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_minimum() {
        assert_eq!(Timeout::new(1).unwrap().as_seconds(), 1);
    }

    #[test]
    fn accepts_maximum() {
        assert_eq!(Timeout::new(300).unwrap().as_seconds(), 300);
    }

    #[test]
    fn rejects_small_timeout() {
        assert_eq!(Timeout::new(0), Err(DomainError::TimeoutTooSmall));
    }

    #[test]
    fn rejects_large_timeout() {
        assert_eq!(Timeout::new(301), Err(DomainError::TimeoutTooLarge));
    }
}
