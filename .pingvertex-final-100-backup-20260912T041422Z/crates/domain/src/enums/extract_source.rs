use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtractSource {
    JsonPath(String),
    Header(String),
    StatusCode,
    Body,
}
