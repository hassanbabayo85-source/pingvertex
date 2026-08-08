use super::assertion_evaluation_result::AssertionEvaluationResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub overall_status: bool,
    pub passed_count: usize,
    pub failed_count: usize,
    pub individual_results: Vec<AssertionEvaluationResult>,
    pub execution_duration_ms: u64,
}
