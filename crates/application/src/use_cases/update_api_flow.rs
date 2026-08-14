use std::error::Error;

use chrono::Utc;

use crate::{
    commands::UpdateApiFlow,
    dto::{ApiFlowStepInput, ExtractSourceInput},
    ports::{ApiFlowRepository, MonitorRepository},
};
use domain::{
    ExtractSource,
    entities::{ApiFlow, ApiFlowStep, Assertion},
    value_objects::{FlowStepName, VariableExtraction},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub struct UpdateApiFlowUseCase<R, M> {
    api_flow_repository: R,
    monitor_repository: M,
}

impl<R, M> UpdateApiFlowUseCase<R, M>
where
    R: ApiFlowRepository,
    M: MonitorRepository,
{
    pub fn new(api_flow_repository: R, monitor_repository: M) -> Self {
        Self {
            api_flow_repository,
            monitor_repository,
        }
    }

    pub async fn execute(&self, command: UpdateApiFlow) -> UseCaseResult<Option<ApiFlow>> {
        if self
            .monitor_repository
            .find_by_id_for_user(command.monitor_id, command.user_id)
            .await?
            .is_none()
        {
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

        let mut flow = ApiFlow::new(command.id, command.monitor_id, steps)
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        flow.created_at = existing.created_at;
        flow.updated_at = Utc::now();

        self.api_flow_repository.update(&flow).await?;

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
