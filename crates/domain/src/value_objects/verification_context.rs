use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationContext {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub parsed_json: Option<serde_json::Value>,
    pub latency_ms: u64,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
}
