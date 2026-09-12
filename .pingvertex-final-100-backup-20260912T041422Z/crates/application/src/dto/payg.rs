use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API input deliberately contains no price, total, currency, or unit_price.
/// `deny_unknown_fields` makes attempts to smuggle client-controlled financial
/// values fail deserialization instead of silently ignoring them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeterUsageRequest {
    pub operation: String,
    pub units: i64,
    pub request_id: Uuid,
}
