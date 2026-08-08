use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// A validated monitor name.
///
/// Rules:
/// - Cannot be empty.
/// - Leading and trailing whitespace is removed.
/// - Maximum length is 100 characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorName(String);

impl MonitorName {
    /// Creates a validated monitor name.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::EmptyMonitorName);
        }

        if value.chars().count() > 100 {
            return Err(DomainError::MonitorNameTooLong);
        }

        Ok(Self(value.to_owned()))
    }

    /// Returns the inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for MonitorName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_name() {
        let name = MonitorName::new("My API").unwrap();

        assert_eq!(name.as_str(), "My API");
    }

    #[test]
    fn trims_whitespace() {
        let name = MonitorName::new("   My API   ").unwrap();

        assert_eq!(name.as_str(), "My API");
    }

    #[test]
    fn rejects_empty_name() {
        let result = MonitorName::new("");

        assert_eq!(result, Err(DomainError::EmptyMonitorName));
    }

    #[test]
    fn rejects_whitespace_only() {
        let result = MonitorName::new("      ");

        assert_eq!(result, Err(DomainError::EmptyMonitorName));
    }

    #[test]
    fn rejects_name_longer_than_100_chars() {
        let value = "a".repeat(101);

        let result = MonitorName::new(value);

        assert_eq!(result, Err(DomainError::MonitorNameTooLong));
    }
}
