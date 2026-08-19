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
    shared::types::AuthenticatedUser,
    value_objects::{FlowStepName, VariableExtraction},
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

        let steps = build_steps(command.steps)?;

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

fn build_steps(input_steps: Vec<ApiFlowStepInput>) -> UseCaseResult<Vec<ApiFlowStep>> {
    let mut steps = Vec::with_capacity(input_steps.len());

    for step_input in input_steps {
        let name =
            FlowStepName::new(step_input.name).map_err(Box::<dyn Error + Send + Sync>::from)?;

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

        let step = ApiFlowStep::new(
            step_input.order,
            name,
            step_input.method,
            step_input.url_template,
            step_input.headers,
            step_input.body,
            extract,
            assertions,
        )
        .map_err(Box::<dyn Error + Send + Sync>::from)?;

        steps.push(step);
    }

    Ok(steps)
}
