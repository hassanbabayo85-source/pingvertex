use domain::enums::{AssertionOperator, ResponsePart};
use serde::Deserialize;

/// Represents an assertion as received from API clients.
#[derive(Debug, Clone, Deserialize)]
pub struct AssertionInput {
    pub response_part: ResponsePart,
    pub operator: AssertionOperator,
    pub target: Option<String>,
    pub expected_value: Option<String>,
}
