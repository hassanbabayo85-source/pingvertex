use crate::enums::AnonymityLevel;
use serde::{Deserialize, Serialize};

/// Safe, client-facing result data for one proxy check.
/// No credentials or decrypted secret material is ever represented here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyResult {
    pub host: String,
    pub port: u16,
    pub scheme: String,
    pub reachable: bool,
    pub latency_ms: Option<u32>,
    pub supports_https: bool,
    pub country_code: Option<String>,
    pub anonymity: Option<AnonymityLevel>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}
