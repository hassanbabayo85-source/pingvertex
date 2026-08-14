//! HTTP-facing DTOs for the "API Flow" feature.
//!
//! Kept in a separate file from `handlers::types` (rather than folded into
//! that already-large file, and rather than in `application::dto` as
//! originally assumed) because HTTP-facing request/response shapes for
//! this codebase consistently live in `api::handlers` — see
//! `handlers::types::{MonitorResponse, MonitorWriteRequest}` — never in
//! the `application` crate, which only holds the internal
//! `CreateApiFlowRequest` / `ApiFlowStepInput` / etc. these types convert
//! into.

use application::dto::{
    ApiFlowStepInput, AssertionInput, CreateApiFlowRequest, ExtractSourceInput,
    VariableExtractionInput,
};
use domain::shared::ids::MonitorId;
use domain::HttpMethod;
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

#[derive(Debug, Deserialize)]
pub struct ApiFlowStepHttpInput {
    pub order: u16,
    pub name: String,
    /// Deserializes `domain::HttpMethod` directly (its own `Deserialize`
    /// impl handles the "GET"/"HEAD" UPPERCASE JSON strings) — mirrors
    /// `handlers::types::MonitorWriteRequest.method`, which does the same
    /// rather than going through a raw `String` + manual parse step.
    pub method: HttpMethod,
    pub url_template: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    #[serde(default)]
    pub extract: Vec<VariableExtractionHttpInput>,
    /// Reuses `application::dto::AssertionInput` directly — it already
    /// derives `Deserialize` and is used the exact same way by
    /// `handlers::types::MonitorWriteRequest.assertions`. No separate
    /// "AssertionHttpInput" exists in this codebase, so none is created
    /// here either.
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
            headers: value.headers,
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
    /// not the JSON body. Rust's `TryFrom` trait has no way to accept an
    /// extra runtime parameter alongside `Self`, so this is a plain method
    /// (`.try_into_request(monitor_id)`) rather than a `TryFrom` impl —
    /// still a single, simple call site for the Prompt 11 handler.
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
        // `VariableExtraction`'s fields are private; use its `.name()` /
        // `.source()` accessor methods instead (confirmed by the compiler,
        // not an assumption anymore).
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
            // `ResponsePart`/`AssertionOperator` are not `Copy` (confirmed
            // by the compiler), so these are cloned rather than moved out
            // of the shared reference.
            response_part: value.response_part.clone(),
            operator: value.operator.clone(),
            target: value.target.clone(),
            expected_value: value.expected_value.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiFlowStepHttpResponse {
    pub order: u16,
    pub name: String,
    pub method: HttpMethod,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
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
            headers: step.headers.clone(),
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
            // `ApiFlowId`'s own `Display` impl (added in Prompt 1).
            id: flow.id.to_string(),
            monitor_id: flow.monitor_id.0.to_string(),
            steps: flow.steps.iter().map(Into::into).collect(),
            created_at: flow.created_at.to_rfc3339(),
            updated_at: flow.updated_at.to_rfc3339(),
        }
    }
}
