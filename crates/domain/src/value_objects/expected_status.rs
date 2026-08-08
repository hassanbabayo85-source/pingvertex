use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// Validated expected HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedStatus(u16);

impl ExpectedStatus {
    pub const MIN: u16 = 100;
    pub const MAX: u16 = 599;

    /// Creates a validated expected status code.
    pub fn new(code: u16) -> Result<Self, DomainError> {
        if !(Self::MIN..=Self::MAX).contains(&code) {
            return Err(DomainError::InvalidExpectedStatus);
        }

        Ok(Self(code))
    }

    /// Returns the HTTP status code.
    pub fn as_u16(self) -> u16 {
        self.0
    }
}

impl From<ExpectedStatus> for u16 {
    fn from(value: ExpectedStatus) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_200() {
        assert_eq!(ExpectedStatus::new(200).unwrap().as_u16(), 200);
    }

    #[test]
    fn accepts_404() {
        assert_eq!(ExpectedStatus::new(404).unwrap().as_u16(), 404);
    }

    #[test]
    fn rejects_99() {
        assert_eq!(
            ExpectedStatus::new(99),
            Err(DomainError::InvalidExpectedStatus)
        );
    }

    #[test]
    fn rejects_600() {
        assert_eq!(
            ExpectedStatus::new(600),
            Err(DomainError::InvalidExpectedStatus)
        );
    }
}
