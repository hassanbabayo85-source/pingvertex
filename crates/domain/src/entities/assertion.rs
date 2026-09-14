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
        {
            if pattern.len() > MAX_REGEX_PATTERN_LENGTH {
                return Err(format!(
                    "regex pattern must not exceed {MAX_REGEX_PATTERN_LENGTH} characters"
                ));
            }

            // PART 10 — the pattern must actually compile. This moves the
            // "this is an invalid regex" failure from the worker hot path
            // (where it would waste CPU on every check) to the request
            // boundary, where the client can fix it and retry. We do NOT
            // include the pattern itself in the error message, so a request
            // that fails validation cannot echo the pattern back through
            // logs or error responses.
            regex::Regex::new(pattern)
                .map_err(|_| "regex pattern is not a valid regular expression".to_string())?;
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

#[cfg(test)]
mod part10_tests {
    use super::*;
    use crate::enums::{AssertionOperator, ResponsePart};

    #[test]
    fn accepts_a_valid_regex_pattern() {
        let result = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(r"^[a-z]+@[a-z]+\.[a-z]{2,}$".to_string()),
        );
        assert!(result.is_ok(), "valid regex must be accepted at write time");
    }

    #[test]
    fn rejects_an_invalid_regex_pattern() {
        let result = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            // Unclosed character class — invalid syntax.
            Some("[unclosed".to_string()),
        );
        let err = result.expect_err("invalid regex must be rejected at write time");
        assert!(
            err.contains("not a valid regular expression"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_an_oversized_regex_pattern() {
        let pattern = "a".repeat(MAX_REGEX_PATTERN_LENGTH + 1);
        let result = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(pattern),
        );
        let err = result.expect_err("oversized regex must be rejected");
        assert!(err.contains("must not exceed"), "unexpected error: {err}");
    }

    #[test]
    fn non_regex_operators_are_not_compiled() {
        // A string that would be an invalid regex must still be accepted
        // for non-RegexMatch operators — the validator only compiles when
        // the operator is RegexMatch.
        let result = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::Equals,
            None,
            Some("[unclosed".to_string()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn error_message_never_contains_the_pattern() {
        // The pattern is user-supplied; it must not leak into the error
        // string (which callers may log or return to the client).
        let secret = "SECRET_TOKEN_abc123[unclosed";
        let result = Assertion::new(
            ResponsePart::Body,
            AssertionOperator::RegexMatch,
            None,
            Some(secret.to_string()),
        );
        let err = result.expect_err("invalid regex must be rejected");
        assert!(
            !err.contains("SECRET_TOKEN_abc123"),
            "error message leaked the pattern: {err}"
        );
    }
}
