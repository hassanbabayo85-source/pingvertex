use std::{error::Error, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::UpdateApiFlow,
    dto::{ApiFlowStepInput, ExtractSourceInput},
    ports::{ApiFlowRepository, AuditLogEntryRepository, MonitorRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, ExtractSource,
    entities::{ApiFlow, ApiFlowStep, Assertion},
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
    value_objects::{FlowHeader, FlowStepName, VariableExtraction},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateApiFlowUseCase<R, M> {
    api_flow_repository: R,
    monitor_repository: M,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, M> UpdateApiFlowUseCase<R, M>
where
    R: ApiFlowRepository,
    M: MonitorRepository,
{
    pub fn new(
        api_flow_repository: R,
        monitor_repository: M,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            api_flow_repository,
            monitor_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: UpdateApiFlow,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<ApiFlow>> {
        authenticated_user.require(Permission::UpdateMonitor)?;

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let monitor = match self
            .monitor_repository
            .find_by_id(command.monitor_id)
            .await?
        {
            Some(monitor) => monitor,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Ok(None);
        }

        let existing = match self.api_flow_repository.find_by_id(command.id).await? {
            Some(flow) => flow,
            None => return Ok(None),
        };

        if existing.monitor_id != command.monitor_id {
            return Ok(None);
        }

        // The existing flow is passed in so the original secrets can be carried over when ***REDACTED*** is submitted.
        let steps = build_steps(command.steps, &existing)?;

        let mut flow = ApiFlow::new(command.id, command.monitor_id, tenant_id, steps)
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        flow.created_at = existing.created_at;
        flow.updated_at = Utc::now();

        self.api_flow_repository.update(&flow).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::ApiFlowUpdated,
            target_id: flow.id.0.to_string(),
            metadata: json!({
                "monitor_id": flow.monitor_id.0.to_string(),
                "steps": flow.steps.len(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(Some(flow))
    }
}

fn build_steps(
    input_steps: Vec<ApiFlowStepInput>,
    existing_flow: &ApiFlow,
) -> UseCaseResult<Vec<ApiFlowStep>> {
    let mut steps = Vec::with_capacity(input_steps.len());

    for step_input in input_steps {
        let name =
            FlowStepName::new(step_input.name).map_err(Box::<dyn Error + Send + Sync>::from)?;

        let mut headers = Vec::with_capacity(step_input.headers.len());
        for header_input in step_input.headers {
            let mut value = header_input.value;

            // MATAKIN 4 — Response Redaction: Prevent overwriting real secrets with the redacted marker
            if header_input.secret && value == "***REDACTED***" {
                let mut found = false;
                if let Some(existing_step) = existing_flow
                    .steps()
                    .iter()
                    .find(|s| s.order == step_input.order)
                {
                    if let Some(existing_header) = existing_step
                        .headers
                        .iter()
                        .find(|h| h.name() == header_input.name)
                    {
                        value = existing_header.value().to_string();
                        found = true;
                    }
                }
                if !found {
                    return Err(format!(
                        "header '{}' not found in existing step {} to preserve secret",
                        header_input.name, step_input.order
                    )
                    .into());
                }
            }

            let header = FlowHeader::new(header_input.name, value, header_input.secret)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;
            headers.push(header);
        }

        let mut extract = Vec::with_capacity(step_input.extract.len());
        for extraction_input in step_input.extract {
            let source = match extraction_input.source {
                ExtractSourceInput::StatusCode => ExtractSource::StatusCode,
                ExtractSourceInput::Header(name) => ExtractSource::Header(name),
                ExtractSourceInput::JsonPath(path) => ExtractSource::JsonPath(path),
                ExtractSourceInput::Body => ExtractSource::Body,
            };

            let extraction = VariableExtraction::new(extraction_input.name, source)
                .map_err(Box::<dyn Error + Send + Sync>::from)?;

            extract.push(extraction);
        }

        let mut assertions = Vec::with_capacity(step_input.assertions.len());
        for input in &step_input.assertions {
            let assertion = Assertion::new(
                input.response_part.clone(),
                input.operator.clone(),
                input.target.clone(),
                input.expected_value.clone(),
            )
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

            assertions.push(assertion);
        }

        // MATAKIN 6 — Strict Input Limits: Enforced centrally at domain boundary
        let step = ApiFlowStep::new(
            step_input.order,
            name,
            step_input.method,
            step_input.url_template,
            headers,
            step_input.body,
            extract,
            assertions,
        )
        .map_err(Box::<dyn Error + Send + Sync>::from)?;

        steps.push(step);
    }

    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::FlowHeaderInput;
    use domain::{
        HttpMethod,
        shared::ids::{ApiFlowId, MonitorId, TenantId},
        value_objects::{FlowHeader, FlowStepName},
    };

    fn dummy_existing_flow() -> ApiFlow {
        let step1 = ApiFlowStep::new(
            1,
            FlowStepName::new("Login").unwrap(),
            HttpMethod::Get,
            "https://example.com".into(),
            vec![
                FlowHeader::secret("Authorization", "Bearer REAL_SECRET").unwrap(),
                FlowHeader::plain("X-Plain", "plain_value").unwrap(),
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
            vec![step1],
        )
        .unwrap()
    }

    #[test]
    fn build_steps_preserves_existing_secret_when_redacted_is_sent() {
        let existing = dummy_existing_flow();
        let input_steps = vec![ApiFlowStepInput {
            order: 1,
            name: "Login".into(),
            method: HttpMethod::Get,
            url_template: "https://example.com".into(),
            headers: vec![FlowHeaderInput {
                name: "Authorization".into(),
                value: "***REDACTED***".into(),
                secret: true,
            }],
            body: None,
            extract: vec![],
            assertions: vec![],
        }];

        let steps = build_steps(input_steps, &existing).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].headers[0].value(),
            "Bearer REAL_SECRET",
            "Original secret must be preserved"
        );
    }

    #[test]
    fn build_steps_rejects_redacted_when_no_existing_secret() {
        let existing = dummy_existing_flow();
        let input_steps = vec![ApiFlowStepInput {
            order: 2, // New step, ba a taba kirkirarsa ba
            name: "Fetch".into(),
            method: HttpMethod::Get,
            url_template: "https://example.com/data".into(),
            headers: vec![FlowHeaderInput {
                name: "Authorization".into(),
                value: "***REDACTED***".into(),
                secret: true,
            }],
            body: None,
            extract: vec![],
            assertions: vec![],
        }];

        let err = build_steps(input_steps, &existing).unwrap_err();
        assert!(err.to_string().contains("not found in existing step"));
    }
}
