use crate::shared::ids::AssertionId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionEvaluationResult {
    pub assertion_id: AssertionId,
    pub passed: bool,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub message: Option<String>,
}
