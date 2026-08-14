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
            && target.len() > MAX_ASSERTION_STRING_LENGTH {
                return Err(format!(
                    "assertion target must not exceed {MAX_ASSERTION_STRING_LENGTH} characters"
                ));
            }

        if let Some(expected_value) = &self.expected_value
            && expected_value.len() > MAX_ASSERTION_STRING_LENGTH {
                return Err(format!(
                    "assertion expected_value must not exceed {MAX_ASSERTION_STRING_LENGTH} characters"
                ));
            }

        Ok(())
    }
}
