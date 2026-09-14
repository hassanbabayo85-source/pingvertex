#![allow(clippy::collapsible_if)]
use crate::enums::{AssertionOperator, ResponsePart};
use crate::shared::ids::AssertionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Hard cap on the byte length of an assertion's `target` or `expected_value`
/// string.
///
/// Both fields are client-supplied, optional `String`s with no prior limit —
/// a single assertion could carry megabytes of text that would be validated
/// and persisted for no legitimate reason (real assertions compare a JSON
/// path, header name, or short expected string). 2048 mirrors the URL length
/// cap used elsewhere in the domain and comfortably covers real assertions.
pub const MAX_ASSERTION_STRING_LENGTH: usize = 2048;

/// PART 9 — hard cap on the byte length of a regex pattern. Kept smaller
/// than `MAX_ASSERTION_STRING_LENGTH` because regex compilation is
/// expensive and a long pathological pattern can pin a worker thread.
pub const MAX_REGEX_PATTERN_LENGTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub id: AssertionId,
    pub response_part: ResponsePart,
    pub operator: AssertionOperator,
    pub target: Option<String>,
    pub expected_value: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Assertion {
    pub fn new(
        response_part: ResponsePart,
        operator: AssertionOperator,
        target: Option<String>,
        expected_value: Option<String>,
    ) -> Result<Self, String> {
        let now = Utc::now();
        let assertion = Self {
            id: AssertionId::new(),
            response_part,
            operator,
            target,
            expected_value,
            created_at: now,
            updated_at: now,
        };

        assertion.validate()?;
        Ok(assertion)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(target) = &self.target
            && target.len() > MAX_ASSERTION_STRING_LENGTH
        {
            return Err(format!(
                "assertion target must not exceed {MAX_ASSERTION_STRING_LENGTH} characters"
            ));
        }

        if let Some(expected_value) = &self.expected_value
            && expected_value.len() > MAX_ASSERTION_STRING_LENGTH
        {
            return Err(format!(
                "assertion expected_value must not exceed {MAX_ASSERTION_STRING_LENGTH} characters"
            ));
        }

        // PART 9 — regex patterns are compiled on every check; cap the
        // pattern length separately so a pathological pattern cannot pin
        // a worker thread or blow up the regex engine's memory.
        if matches!(self.operator, AssertionOperator::RegexMatch)
            && let Some(pattern) = &self.expected_value
            && pattern.len() > MAX_REGEX_PATTERN_LENGTH
        {
            return Err(format!(
                "regex pattern must not exceed {MAX_REGEX_PATTERN_LENGTH} characters"
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod part9_tests {
    use super::*;
    use crate::enums::{AssertionOperator, ResponsePart};

    #[test]
    fn accepts_regex_pattern_at_max() {
        let pattern = "a".repeat(MAX_REGEX_PATTERN_LENGTH);
        let assertion = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(pattern),
        )
        .unwrap();
        assert_eq!(
            assertion.expected_value.unwrap().len(),
            MAX_REGEX_PATTERN_LENGTH
        );
    }

    #[test]
    fn rejects_regex_pattern_over_max() {
        let pattern = "a".repeat(MAX_REGEX_PATTERN_LENGTH + 1);
        let err = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(pattern),
        )
        .unwrap_err();
        assert!(err.contains("regex pattern"));
    }

    #[test]
    fn non_regex_operators_are_not_limited_by_regex_cap() {
        let value = "a".repeat(MAX_REGEX_PATTERN_LENGTH + 1);
        assert!(value.len() <= MAX_ASSERTION_STRING_LENGTH);
        let assertion = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::Equals,
            None,
            Some(value),
        );
        assert!(assertion.is_ok());
    }
}
