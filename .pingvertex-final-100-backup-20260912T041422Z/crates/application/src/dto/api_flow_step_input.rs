use crate::dto::{AssertionInput, VariableExtractionInput};

/// Represents a single API flow step as received from API clients.
#[derive(Debug, Clone)]
pub struct ApiFlowStepInput {
    pub order: u16,
    pub name: String,
    pub method: domain::HttpMethod,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub extract: Vec<VariableExtractionInput>,
    pub assertions: Vec<AssertionInput>,
}
