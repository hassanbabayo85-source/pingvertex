use crate::enums::AnonymityLevel;
use serde::{Deserialize, Serialize};

/// The outcome of checking a single proxy entry.
///
/// This is a plain data struct, not a validated value object: it holds the
/// *result* of a check performed by the worker layer, not user-supplied
/// input, so there is no domain validation to enforce here. There is no
/// secret material in this type either (unlike `ProxyCredentials`), so it
/// derives `Debug`/`Serialize`/`Deserialize` normally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResult {
    pub reachable: bool,
    pub latency_ms: Option<u32>,
    pub anonymity: Option<AnonymityLevel>,
    pub supports_https: bool,
    pub country_code: Option<String>,
    pub error: Option<String>,
}
