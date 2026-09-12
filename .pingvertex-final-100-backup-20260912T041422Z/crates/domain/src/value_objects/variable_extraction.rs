use serde::{Deserialize, Serialize};

use crate::enums::ExtractSource;
use crate::errors::DomainError;

/// Hard cap on the byte length of a `VariableExtraction`'s `name`.
///
/// The name is client-supplied with no prior limit — a single extraction
/// could carry megabytes of text that would be validated and persisted for
/// no legitimate reason (a real extraction name is a short identifier used
/// as a HashMap key, e.g. `auth_token`). 64 comfortably covers real names
/// while closing off the DoS/no-limit vector.
pub const MAX_NAME_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableExtraction {
    name: String,
    source: ExtractSource,
}

impl VariableExtraction {
    /// Creates a validated variable extraction.
    pub fn new(name: impl Into<String>, source: ExtractSource) -> Result<Self, DomainError> {
        let name = name.into();
        let name = name.trim();

        if name.is_empty() {
            return Err(DomainError::EmptyExtractionName);
        }

        if name.chars().count() > MAX_NAME_LENGTH {
            return Err(DomainError::ExtractionNameTooLong);
        }

        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(DomainError::InvalidExtractionNameCharacters);
        }

        Ok(Self {
            name: name.to_owned(),
            source,
        })
    }

    /// Returns the extraction name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the extraction source.
    pub fn source(&self) -> &ExtractSource {
        &self.source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_alphanumeric_underscore_name() {
        let extraction = VariableExtraction::new(
            "access_token_1",
            ExtractSource::JsonPath("$.token".to_string()),
        )
        .unwrap();

        assert_eq!(extraction.name(), "access_token_1");
    }

    #[test]
    fn trims_whitespace() {
        let extraction = VariableExtraction::new("  access_token  ", ExtractSource::Body).unwrap();

        assert_eq!(extraction.name(), "access_token");
    }

    #[test]
    fn rejects_empty_name() {
        let result = VariableExtraction::new("", ExtractSource::Body);

        assert_eq!(result, Err(DomainError::EmptyExtractionName));
    }

    #[test]
    fn rejects_whitespace_only_name() {
        let result = VariableExtraction::new("      ", ExtractSource::Body);

        assert_eq!(result, Err(DomainError::EmptyExtractionName));
    }

    #[test]
    fn rejects_name_longer_than_64_chars() {
        let value = "a".repeat(65);

        let result = VariableExtraction::new(value, ExtractSource::Body);

        assert_eq!(result, Err(DomainError::ExtractionNameTooLong));
    }

    #[test]
    fn rejects_name_with_dot() {
        let result = VariableExtraction::new(
            "access.token",
            ExtractSource::JsonPath("$.token".to_string()),
        );

        assert_eq!(result, Err(DomainError::InvalidExtractionNameCharacters));
    }

    #[test]
    fn rejects_name_with_dash() {
        let result = VariableExtraction::new(
            "access-token",
            ExtractSource::JsonPath("$.token".to_string()),
        );

        assert_eq!(result, Err(DomainError::InvalidExtractionNameCharacters));
    }

    #[test]
    fn rejects_name_with_spaces() {
        let result = VariableExtraction::new(
            "access token",
            ExtractSource::JsonPath("$.token".to_string()),
        );

        assert_eq!(result, Err(DomainError::InvalidExtractionNameCharacters));
    }
}
