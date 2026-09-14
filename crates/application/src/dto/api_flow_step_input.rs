use crate::dto::{AssertionInput, FlowHeaderInput, VariableExtractionInput};

/// Represents a single API flow step as received from API clients.
#[derive(Debug, Clone)]
pub struct ApiFlowStepInput {
    pub order: u16,
    pub name: String,
    pub method: domain::HttpMethod,
    pub url_template: String,
    /// Headers with sensitivity classification. See `FlowHeaderInput`'s
    /// `Debug` impl for the redaction guarantee.
    pub headers: Vec<FlowHeaderInput>,
    pub body: Option<String>,
    pub extract: Vec<VariableExtractionInput>,
    pub assertions: Vec<AssertionInput>,
}
