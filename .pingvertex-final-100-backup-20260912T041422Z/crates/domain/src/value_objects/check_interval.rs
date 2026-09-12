use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Validated monitor check interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckInterval(u32);

impl CheckInterval {
    pub const MIN: u32 = 30;
    pub const MAX: u32 = 86_400;

    /// Creates a validated check interval.
    pub fn new(seconds: u32) -> Result<Self, DomainError> {
        if seconds < Self::MIN {
            return Err(DomainError::IntervalTooSmall);
        }

        if seconds > Self::MAX {
            return Err(DomainError::IntervalTooLarge);
        }

        Ok(Self(seconds))
    }

    /// Returns the interval in seconds.
    pub fn as_seconds(self) -> u32 {
        self.0
    }
}

impl From<CheckInterval> for u32 {
    fn from(value: CheckInterval) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_minimum() {
        assert_eq!(CheckInterval::new(30).unwrap().as_seconds(), 30);
    }

    #[test]
    fn accepts_maximum() {
        assert_eq!(CheckInterval::new(86_400).unwrap().as_seconds(), 86_400);
    }

    #[test]
    fn rejects_small_interval() {
        assert_eq!(CheckInterval::new(29), Err(DomainError::IntervalTooSmall));
    }

    #[test]
    fn rejects_large_interval() {
        assert_eq!(
            CheckInterval::new(86_401),
            Err(DomainError::IntervalTooLarge)
        );
    }
}
