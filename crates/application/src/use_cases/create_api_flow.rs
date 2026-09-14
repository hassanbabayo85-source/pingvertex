use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::{CreateApiFlowRequest, ExtractSourceInput},
    ports::{ApiFlowRepository, AuditLogEntryRepository, IdGenerator, MonitorRepository},
};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, ExtractSource,
    entities::{ApiFlow, ApiFlowStep, Assertion},
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
    value_objects::{FlowHeader, FlowStepName, VariableExtraction},
};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum CreateApiFlowError {
    Forbidden,
    MonitorNotFound,
    FlowAlreadyExists,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateApiFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create api flows"
            ),
            Self::MonitorNotFound => write!(f, "monitor not found"),
            Self::FlowAlreadyExists => write!(f, "an api flow already exists for this monitor"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateApiFlowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Forbidden => None,
            Self::MonitorNotFound => None,
            Self::FlowAlreadyExists => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct CreateApiFlowUseCase<R, G, M> {
    repository: R,
    id_generator: G,
    monitor_repository: M,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, G, M> CreateApiFlowUseCase<R, G, M>
where
    R: ApiFlowRepository,
    G: IdGenerator,
    M: MonitorRepository,
{
    pub fn new(
        repository: R,
        id_generator: G,
        monitor_repository: M,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            monitor_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        request: CreateApiFlowRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<ApiFlow, CreateApiFlowError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateApiFlowError::Forbidden);
        }

        let monitor = self
            .monitor_repository
            .find_by_id(request.monitor_id)
            .await
            .map_err(CreateApiFlowError::Other)?
            .ok_or(CreateApiFlowError::MonitorNotFound)?;

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Err(CreateApiFlowError::MonitorNotFound);
        }

        let existing = self
            .repository
            .find_by_monitor_id(request.monitor_id)
            .await
            .map_err(CreateApiFlowError::Other)?;

        if existing.is_some() {
            return Err(CreateApiFlowError::FlowAlreadyExists);
        }

        self.build_and_save(request, authenticated_user)
            .await
            .map_err(CreateApiFlowError::Other)
    }

    async fn build_and_save(
        &self,
        request: CreateApiFlowRequest,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<ApiFlow> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut steps = Vec::with_capacity(request.steps.len());

        for step_input in request.steps {
            let name =
                FlowStepName::new(step_input.name).map_err(Box::<dyn Error + Send + Sync>::from)?;

            let mut headers = Vec::with_capacity(step_input.headers.len());
            for header_input in step_input.headers {
                // MATAKIN 4 & 6 — Validation: Prevent saving the exact string "***REDACTED***"
                // as a literal secret during initial creation.
                if header_input.secret && header_input.value == "***REDACTED***" {
                    return Err(format!(
                        "cannot use '***REDACTED***' for header '{}' during creation",
                        header_input.name
                    )
                    .into());
                }

                let header =
                    FlowHeader::new(header_input.name, header_input.value, header_input.secret)
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

            // MATAKIN 6 — Strict Input Limits: Enforced centrally at domain boundary.
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

        let id = self.id_generator.next_api_flow_id().await;

        let flow = ApiFlow::new(id, request.monitor_id, tenant_id, steps)
            .map_err(Box::<dyn Error + Send + Sync>::from)?;

        self.repository.save(&flow).await?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::ApiFlowCreated,
            target_id: flow.id.0.to_string(),
            metadata: json!({
                "monitor_id": flow.monitor_id.0.to_string(),
                "steps": flow.steps.len(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository.create(&entry).await?;

        Ok(flow)
    }
}
