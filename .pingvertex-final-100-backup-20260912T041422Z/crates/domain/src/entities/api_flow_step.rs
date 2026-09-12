#![allow(clippy::collapsible_if)]
use serde::{Deserialize, Serialize};

use crate::entities::Assertion;
use crate::enums::HttpMethod;
use crate::errors::DomainError;
use crate::shared::ids::ApiFlowStepId;
use crate::value_objects::{FlowStepName, VariableExtraction};

/// Hard cap on the number of headers a single flow step may carry.
///
/// `headers` is a client-supplied, unbounded `Vec<(String, String)>` with no
/// prior limit — a single step could carry thousands of headers that would
/// be validated and persisted for no legitimate reason (a real HTTP request
/// needs only a handful). 20 comfortably covers real use while blocking abuse.
pub const MAX_HEADERS: usize = 20;

/// Hard cap on the byte length of a single header value.
pub const MAX_HEADER_VALUE_LENGTH: usize = 2048;

/// Hard cap on the byte length of the request body.
pub const MAX_BODY_LENGTH: usize = 65536;

/// Hard cap on the byte length of the url template.
pub const MAX_URL_TEMPLATE_LENGTH: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiFlowStep {
    pub id: ApiFlowStepId,
    pub order: u16,
    pub name: FlowStepName,
    pub method: HttpMethod,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub extract: Vec<VariableExtraction>,
    pub assertions: Vec<Assertion>,
}

impl ApiFlowStep {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        order: u16,
        name: FlowStepName,
        method: HttpMethod,
        url_template: String,
        headers: Vec<(String, String)>,
        body: Option<String>,
        extract: Vec<VariableExtraction>,
        assertions: Vec<Assertion>,
    ) -> Result<Self, DomainError> {
        if url_template.trim().is_empty() {
            return Err(DomainError::EmptyUrlTemplate);
        }

        if url_template.len() > MAX_URL_TEMPLATE_LENGTH {
            return Err(DomainError::UrlTemplateTooLong);
        }

        if headers.len() > MAX_HEADERS {
            return Err(DomainError::TooManyHeaders);
        }

        for (_, value) in &headers {
            if value.len() > MAX_HEADER_VALUE_LENGTH {
                return Err(DomainError::HeaderValueTooLong);
            }
        }

        if let Some(body) = &body
            && body.len() > MAX_BODY_LENGTH
        {
            return Err(DomainError::FlowStepBodyTooLarge);
        }

        Ok(Self {
            id: ApiFlowStepId::new(),
            order,
            name,
            method,
            url_template,
            headers,
            body,
            extract,
            assertions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_name() -> FlowStepName {
        FlowStepName::new("Login").unwrap()
    }

    #[test]
    fn accepts_valid_step() {
        let step = ApiFlowStep::new(
            1,
            valid_name(),
            HttpMethod::Get,
            "https://example.com/login".to_string(),
            vec![("Authorization".to_string(), "Bearer token".to_string())],
            Some("{}".to_string()),
            vec![],
            vec![],
        )
        .unwrap();

        assert_eq!(step.order, 1);
        assert_eq!(step.url_template, "https://example.com/login");
    }

    #[test]
    fn rejects_empty_url_template() {
        let result = ApiFlowStep::new(
            1,
            valid_name(),
            HttpMethod::Get,
            "   ".to_string(),
            vec![],
            None,
            vec![],
            vec![],
        );

        assert_eq!(result, Err(DomainError::EmptyUrlTemplate));
    }

    #[test]
    fn rejects_url_template_too_long() {
        let value = "a".repeat(MAX_URL_TEMPLATE_LENGTH + 1);

        let result = ApiFlowStep::new(
            1,
            valid_name(),
            HttpMethod::Get,
            value,
            vec![],
            None,
            vec![],
            vec![],
        );

        assert_eq!(result, Err(DomainError::UrlTemplateTooLong));
    }

    #[test]
    fn rejects_too_many_headers() {
        let headers = (0..(MAX_HEADERS + 1))
            .map(|i| (format!("X-Header-{i}"), "value".to_string()))
            .collect();

        let result = ApiFlowStep::new(
            1,
            valid_name(),
            HttpMethod::Get,
            "https://example.com".to_string(),
            headers,
            None,
            vec![],
            vec![],
        );

        assert_eq!(result, Err(DomainError::TooManyHeaders));
    }

    #[test]
    fn rejects_header_value_too_long() {
        let value = "a".repeat(MAX_HEADER_VALUE_LENGTH + 1);
        let headers = vec![("X-Header".to_string(), value)];

        let result = ApiFlowStep::new(
            1,
            valid_name(),
            HttpMethod::Get,
            "https://example.com".to_string(),
            headers,
            None,
            vec![],
            vec![],
        );

        assert_eq!(result, Err(DomainError::HeaderValueTooLong));
    }

    #[test]
    fn rejects_body_too_large() {
        let body = "a".repeat(MAX_BODY_LENGTH + 1);

        let result = ApiFlowStep::new(
            1,
            valid_name(),
            HttpMethod::Get,
            "https://example.com".to_string(),
            vec![],
            Some(body),
            vec![],
            vec![],
        );

        assert_eq!(result, Err(DomainError::FlowStepBodyTooLarge));
    }
}
