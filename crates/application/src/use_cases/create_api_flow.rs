use std::{error::Error, fmt};

use crate::{
    dto::{CreateApiFlowRequest, ExtractSourceInput},
    ports::{ApiFlowRepository, IdGenerator},
};
use domain::{
    ExtractSource,
    entities::{ApiFlow, ApiFlowStep, Assertion},
    value_objects::{FlowStepName, VariableExtraction},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum CreateApiFlowError {
    /// A monitor may have at most one API flow (1-to-1 relationship). Returned
    /// when `ApiFlowRepository::find_by_monitor_id` already finds one.
    FlowAlreadyExists,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateApiFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FlowAlreadyExists => {
                write!(f, "an api flow already exists for this monitor")
            }
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateApiFlowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FlowAlreadyExists => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateApiFlowUseCase<R, G> {
    repository: R,
    id_generator: G,
}

impl<R, G> CreateApiFlowUseCase<R, G>
where
    R: ApiFlowRepository,
    G: IdGenerator,
{
    pub fn new(repository: R, id_generator: G) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        request: CreateApiFlowRequest,
    ) -> Result<ApiFlow, CreateApiFlowError> {
        let existing = self
            .repository
            .find_by_monitor_id(request.monitor_id)
            .await
            .map_err(CreateApiFlowError::Other)?;

        if existing.is_some() {
            return Err(CreateApiFlowError::FlowAlreadyExists);
        }

        self.build_and_save(request)
            .await
            .map_err(CreateApiFlowError::Other)
    }

    async fn build_and_save(&self, request: CreateApiFlowRequest) -> UseCaseResult<ApiFlow> {
        let mut steps = Vec::with_capacity(request.steps.len());

        for step_input in request.steps {
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

        let id = self.id_generator.next_api_flow_id().await;

        let flow = ApiFlow::new(id, request.monitor_id, steps)
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        self.repository.save(&flow).await?;

        Ok(flow)
    }
}
