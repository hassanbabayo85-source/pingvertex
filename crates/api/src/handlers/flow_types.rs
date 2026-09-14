//! HTTP-facing DTOs for the "API Flow" feature.
//!
//! Kept in a separate file from `handlers::types` (rather than folded into
//! that already-large file, and rather than in `application::dto` as
//! originally assumed) because HTTP-facing request/response shapes for
//! this codebase consistently live in `api::handlers`.

use application::dto::{
    ApiFlowStepInput, AssertionInput, CreateApiFlowRequest, ExtractSourceInput, FlowHeaderInput,
    VariableExtractionInput,
};
use domain::HttpMethod;
use domain::shared::ids::MonitorId;
use domain::value_objects::FlowHeader;
use serde::{Deserialize, Serialize};

// ============================= Request side =============================

/// Tagged union for JSON body clarity, e.g. `{"type": "header", "name": "X-Auth"}`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtractSourceHttpInput {
    StatusCode,
    Header { name: String },
    JsonPath { path: String },
    Body,
}

impl From<ExtractSourceHttpInput> for ExtractSourceInput {
    fn from(value: ExtractSourceHttpInput) -> Self {
        match value {
            ExtractSourceHttpInput::StatusCode => ExtractSourceInput::StatusCode,
            ExtractSourceHttpInput::Header { name } => ExtractSourceInput::Header(name),
            ExtractSourceHttpInput::JsonPath { path } => ExtractSourceInput::JsonPath(path),
            ExtractSourceHttpInput::Body => ExtractSourceInput::Body,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VariableExtractionHttpInput {
    pub name: String,
    pub source: ExtractSourceHttpInput,
}

impl From<VariableExtractionHttpInput> for VariableExtractionInput {
    fn from(value: VariableExtractionHttpInput) -> Self {
        VariableExtractionInput {
            name: value.name,
            source: value.source.into(),
        }
    }
}

/// HTTP-side input for a single header on a flow step.
///
/// Accepts two on-the-wire forms:
///
/// 1. Legacy tuple: `["Authorization", "Bearer abc"]`.
///    Auto-classified as secret if `name` matches a known sensitive
///    header (see `FlowHeader::is_sensitive_name`).
/// 2. Structured: `{"name": "...", "value": "...", "secret": true}`.
///    `secret` is optional; when omitted, the same name-based
///    auto-classification applies.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FlowHeaderHttpInput {
    Tuple((String, String)),
    Structured {
        name: String,
        value: String,
        #[serde(default)]
        secret: Option<bool>,
    },
}

impl FlowHeaderHttpInput {
    fn into_input(self) -> FlowHeaderInput {
        match self {
            FlowHeaderHttpInput::Tuple((name, value)) => {
                let secret = FlowHeader::is_sensitive_name(&name);
                FlowHeaderInput {
                    name,
                    value,
                    secret,
                }
            }
            FlowHeaderHttpInput::Structured {
                name,
                value,
                secret,
            } => {
                let secret = secret.unwrap_or_else(|| FlowHeader::is_sensitive_name(&name));
                FlowHeaderInput {
                    name,
                    value,
                    secret,
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ApiFlowStepHttpInput {
    pub order: u16,
    pub name: String,
    /// Deserializes `domain::HttpMethod` directly (its own `Deserialize`
    /// impl handles the "GET"/"HEAD" UPPERCASE JSON strings).
    pub method: HttpMethod,
    pub url_template: String,
    #[serde(default)]
    pub headers: Vec<FlowHeaderHttpInput>,
    pub body: Option<String>,
    #[serde(default)]
    pub extract: Vec<VariableExtractionHttpInput>,
    #[serde(default)]
    pub assertions: Vec<AssertionInput>,
}

impl TryFrom<ApiFlowStepHttpInput> for ApiFlowStepInput {
    type Error = String;

    fn try_from(value: ApiFlowStepHttpInput) -> Result<Self, Self::Error> {
        Ok(ApiFlowStepInput {
            order: value.order,
            name: value.name,
            method: value.method,
            url_template: value.url_template,
            headers: value
                .headers
                .into_iter()
                .map(FlowHeaderHttpInput::into_input)
                .collect(),
            body: value.body,
            extract: value.extract.into_iter().map(Into::into).collect(),
            assertions: value.assertions,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateApiFlowHttpRequest {
    pub steps: Vec<ApiFlowStepHttpInput>,
}

impl CreateApiFlowHttpRequest {
    /// `monitor_id` comes from the `/monitors/:id/flows` URL path param,
    /// not the JSON body.
    pub fn try_into_request(self, monitor_id: MonitorId) -> Result<CreateApiFlowRequest, String> {
        let steps = self
            .steps
            .into_iter()
            .map(ApiFlowStepInput::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CreateApiFlowRequest { monitor_id, steps })
    }
}

// ============================= Response side =============================

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtractSourceHttpResponse {
    StatusCode,
    Header { name: String },
    JsonPath { path: String },
    Body,
}

impl From<&domain::enums::ExtractSource> for ExtractSourceHttpResponse {
    fn from(value: &domain::enums::ExtractSource) -> Self {
        match value {
            domain::enums::ExtractSource::StatusCode => ExtractSourceHttpResponse::StatusCode,
            domain::enums::ExtractSource::Header(name) => {
                ExtractSourceHttpResponse::Header { name: name.clone() }
            }
            domain::enums::ExtractSource::JsonPath(path) => {
                ExtractSourceHttpResponse::JsonPath { path: path.clone() }
            }
            domain::enums::ExtractSource::Body => ExtractSourceHttpResponse::Body,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VariableExtractionHttpResponse {
    pub name: String,
    pub source: ExtractSourceHttpResponse,
}

impl From<&domain::value_objects::VariableExtraction> for VariableExtractionHttpResponse {
    fn from(value: &domain::value_objects::VariableExtraction) -> Self {
        VariableExtractionHttpResponse {
            name: value.name().to_string(),
            source: value.source().into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AssertionHttpResponse {
    pub id: String,
    pub response_part: domain::enums::ResponsePart,
    pub operator: domain::enums::AssertionOperator,
    pub target: Option<String>,
    pub expected_value: Option<String>,
}

impl From<&domain::entities::Assertion> for AssertionHttpResponse {
    fn from(value: &domain::entities::Assertion) -> Self {
        AssertionHttpResponse {
            id: value.id.0.to_string(),
            response_part: value.response_part.clone(),
            operator: value.operator.clone(),
            target: value.target.clone(),
            expected_value: value.expected_value.clone(),
        }
    }
}

/// HTTP-side output for a single header on a flow step.
///
/// SECURITY: `value` is `"***REDACTED***"` for secret headers — the
/// plaintext is never serialized into an HTTP response. For non-secret
/// headers, `value` is the plaintext header value.
///
/// `secret` tells the client whether the value it is receiving is the
/// real value or a placeholder. A client updating the flow must send a
/// fresh plaintext value for secret headers; the redacted string
/// `"***REDACTED***"` is a display-only marker and is not accepted back
/// as a password — it would be stored as the literal string if echoed.
#[derive(Debug, Serialize)]
pub struct FlowHeaderHttpResponse {
    pub name: String,
    pub secret: bool,
    pub value: String,
}

impl From<&FlowHeader> for FlowHeaderHttpResponse {
    fn from(value: &FlowHeader) -> Self {
        let rendered = if value.is_secret() {
            "***REDACTED***".to_string()
        } else {
            value.value().to_string()
        };
        FlowHeaderHttpResponse {
            name: value.name().to_string(),
            secret: value.is_secret(),
            value: rendered,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiFlowStepHttpResponse {
    pub order: u16,
    pub name: String,
    pub method: HttpMethod,
    pub url_template: String,
    pub headers: Vec<FlowHeaderHttpResponse>,
    pub body: Option<String>,
    pub extract: Vec<VariableExtractionHttpResponse>,
    pub assertions: Vec<AssertionHttpResponse>,
}

impl From<&domain::entities::ApiFlowStep> for ApiFlowStepHttpResponse {
    fn from(step: &domain::entities::ApiFlowStep) -> Self {
        ApiFlowStepHttpResponse {
            order: step.order,
            name: step.name.as_str().to_string(),
            method: step.method,
            url_template: step.url_template.clone(),
            headers: step.headers.iter().map(Into::into).collect(),
            body: step.body.clone(),
            extract: step.extract.iter().map(Into::into).collect(),
            assertions: step.assertions.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiFlowHttpResponse {
    pub id: String,
    pub monitor_id: String,
    pub steps: Vec<ApiFlowStepHttpResponse>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<domain::entities::ApiFlow> for ApiFlowHttpResponse {
    fn from(flow: domain::entities::ApiFlow) -> Self {
        ApiFlowHttpResponse {
            id: flow.id.to_string(),
            monitor_id: flow.monitor_id.0.to_string(),
            steps: flow.steps.iter().map(Into::into).collect(),
            created_at: flow.created_at.to_rfc3339(),
            updated_at: flow.updated_at.to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::{ApiFlow, ApiFlowStep};
    use domain::shared::ids::{ApiFlowId, MonitorId, TenantId};
    use domain::value_objects::FlowStepName;

    fn flow_with_secret_headers() -> ApiFlow {
        let step = ApiFlowStep::new(
            1,
            FlowStepName::new("Login").unwrap(),
            HttpMethod::Get,
            "https://example.com".to_string(),
            vec![
                FlowHeader::secret("Authorization", "Bearer SUPER_SECRET_TOKEN").unwrap(),
                FlowHeader::secret("Cookie", "session=SUPER_SECRET_COOKIE").unwrap(),
                FlowHeader::secret("X-Api-Key", "SUPER_SECRET_API_KEY").unwrap(),
                FlowHeader::plain("Content-Type", "application/json").unwrap(),
            ],
            None,
            vec![],
            vec![],
        )
        .unwrap();

        ApiFlow::new(
            ApiFlowId::new(),
            MonitorId::new(),
            TenantId::new(),
            vec![step],
        )
        .unwrap()
    }

    fn assert_no_plaintext(json: &str, needles: &[&str], ctx: &str) {
        for needle in needles {
            assert!(
                !needle.is_empty(),
                "test bug: an empty needle would pass vacuously"
            );
            assert!(
                !json.contains(needle),
                "SECURITY: {ctx} leaked plaintext '{needle}' in: {json}"
            );
        }
    }

    #[test]
    fn get_response_never_contains_secret_header_values() {
        let flow = flow_with_secret_headers();
        let response = ApiFlowHttpResponse::from(flow);
        let json = serde_json::to_string(&response).unwrap();

        assert_no_plaintext(
            &json,
            &[
                "SUPER_SECRET_TOKEN",
                "SUPER_SECRET_COOKIE",
                "SUPER_SECRET_API_KEY",
            ],
            "ApiFlowHttpResponse",
        );
        // Non-secret headers still round-trip.
        assert!(json.contains("application/json"));
        // Redacted marker present.
        assert!(json.contains("***REDACTED***"));
    }

    #[test]
    fn list_response_never_contains_secret_header_values() {
        // The list endpoint reuses `ApiFlowHttpResponse` per element.
        let flow = flow_with_secret_headers();
        let response = vec![ApiFlowHttpResponse::from(flow)];
        let json = serde_json::to_string(&response).unwrap();
        assert_no_plaintext(
            &json,
            &[
                "SUPER_SECRET_TOKEN",
                "SUPER_SECRET_COOKIE",
                "SUPER_SECRET_API_KEY",
            ],
            "list ApiFlowHttpResponse",
        );
    }

    #[test]
    fn create_response_never_contains_secret_header_values() {
        // Create/update/get/list all go through `ApiFlowHttpResponse`.
        let flow = flow_with_secret_headers();
        let response = ApiFlowHttpResponse::from(flow);
        let json = serde_json::to_string(&response).unwrap();
        assert_no_plaintext(
            &json,
            &[
                "SUPER_SECRET_TOKEN",
                "SUPER_SECRET_COOKIE",
                "SUPER_SECRET_API_KEY",
            ],
            "create/update ApiFlowHttpResponse",
        );
    }

    #[test]
    fn input_auto_classifies_authorization_header_as_secret() {
        let raw: FlowHeaderHttpInput =
            serde_json::from_str(r#"["Authorization", "Bearer SECRET"]"#).unwrap();
        let input = raw.into_input();
        assert!(input.secret, "Authorization must auto-classify as secret");
    }

    #[test]
    fn input_auto_classifies_cookie_header_as_secret() {
        let raw: FlowHeaderHttpInput =
            serde_json::from_str(r#"["Cookie", "session=SECRET"]"#).unwrap();
        let input = raw.into_input();
        assert!(input.secret, "Cookie must auto-classify as secret");
    }

    #[test]
    fn input_auto_classifies_x_api_key_header_as_secret() {
        let raw: FlowHeaderHttpInput = serde_json::from_str(r#"["X-API-Key", "SECRET"]"#).unwrap();
        let input = raw.into_input();
        assert!(input.secret, "X-API-Key must auto-classify as secret");
    }

    #[test]
    fn input_accepts_structured_secret_form() {
        let raw: FlowHeaderHttpInput =
            serde_json::from_str(r#"{"name":"X-Custom","value":"v","secret":true}"#).unwrap();
        let input = raw.into_input();
        assert!(input.secret);
    }

    #[test]
    fn input_structured_form_without_secret_field_auto_classifies() {
        let raw: FlowHeaderHttpInput =
            serde_json::from_str(r#"{"name":"Authorization","value":"Bearer x"}"#).unwrap();
        let input = raw.into_input();
        assert!(input.secret);
    }

    #[test]
    fn input_explicit_secret_false_overrides_name_default() {
        let raw: FlowHeaderHttpInput =
            serde_json::from_str(r#"{"name":"Authorization","value":"none","secret":false}"#)
                .unwrap();
        let input = raw.into_input();
        assert!(
            !input.secret,
            "explicit secret=false wins over name default"
        );
    }

    #[test]
    fn input_plain_header_stays_non_secret() {
        let raw: FlowHeaderHttpInput =
            serde_json::from_str(r#"["Content-Type", "application/json"]"#).unwrap();
        let input = raw.into_input();
        assert!(!input.secret);
    }
}
