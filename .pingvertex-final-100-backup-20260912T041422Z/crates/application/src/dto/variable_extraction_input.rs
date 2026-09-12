/// DTO version of `domain::ExtractSource`, kept independent so the
/// application-layer request boundary does not depend on the domain enum
/// directly (mirrors how other request DTOs avoid exposing domain types).
#[derive(Debug, Clone)]
pub enum ExtractSourceInput {
    StatusCode,
    Header(String),
    JsonPath(String),
    Body,
}

/// Represents a variable extraction as received from API clients.
#[derive(Debug, Clone)]
pub struct VariableExtractionInput {
    pub name: String,
    pub source: ExtractSourceInput,
}
