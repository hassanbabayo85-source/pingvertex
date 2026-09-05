use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::ports::monitor_repository::QuotaCheckOutcome;
use crate::{
    dto::CreateProxyCheckJobRequest,
    ports::{AuditLogEntryRepository, IdGenerator, ProxyCheckJobRepository},
};
use domain::shared::policies::AuthorizationPolicy;
use domain::shared::types::AuthenticatedUser;
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, DomainError, ProxyCheckJob, ProxyCredentials,
    ProxyEntry, ProxyHost,
};

pub const MAX_JOBS_PER_USER_PER_DAY: usize = 5;

#[derive(Debug)]
pub enum SubmitProxyCheckJobError {
    QuotaExceeded { limit: usize },
    InvalidInput(String),
    DomainError(DomainError),
    RepoError(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for SubmitProxyCheckJobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => write!(
                f,
                "daily proxy check job limit reached: at most {limit} jobs may be submitted per day"
            ),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::DomainError(err) => write!(f, "{err}"),
            Self::RepoError(err) => write!(f, "{err}"),
        }
    }
}

impl Error for SubmitProxyCheckJobError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::QuotaExceeded { .. } => None,
            Self::InvalidInput(_) => None,
            Self::DomainError(err) => Some(err),
            Self::RepoError(err) => Some(err.as_ref()),
        }
    }
}

pub struct SubmitProxyCheckJobUseCase<R, G> {
    repository: R,
    id_generator: G,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R, G> SubmitProxyCheckJobUseCase<R, G>
where
    R: ProxyCheckJobRepository,
    G: IdGenerator,
{
    pub fn new(
        repository: R,
        id_generator: G,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            id_generator,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        request: CreateProxyCheckJobRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<ProxyCheckJob, SubmitProxyCheckJobError> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        if request.entries.is_empty() {
            return Err(SubmitProxyCheckJobError::InvalidInput(
                "at least one proxy entry is required".to_string(),
            ));
        }

        let job_id = self.id_generator.next_proxy_check_job_id().await;

        let total_count = u32::try_from(request.entries.len())
            .map_err(|_| SubmitProxyCheckJobError::InvalidInput("too many entries".to_string()))?;

        let job = ProxyCheckJob::new(job_id, user_id, tenant_id, total_count);

        debug_assert!(
            AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), job.tenant_id()),
            "newly created proxy check job must belong to the authenticated user's tenant"
        );

        let mut entries = Vec::with_capacity(request.entries.len());
        for input in &request.entries {
            let entry_id = self.id_generator.next_proxy_entry_id().await;

            let host = ProxyHost::new(input.host.clone())
                .map_err(SubmitProxyCheckJobError::DomainError)?;

            let credentials = match (&input.username, &input.password) {
                (None, None) => None,
                (username, password) => Some(
                    ProxyCredentials::new(
                        username.clone().unwrap_or_default(),
                        password.clone().unwrap_or_default(),
                    )
                    .map_err(SubmitProxyCheckJobError::DomainError)?,
                ),
            };

            let entry = ProxyEntry::new(
                entry_id,
                job_id,
                host,
                input.port,
                input.scheme.clone(),
                credentials,
            )
            .map_err(SubmitProxyCheckJobError::DomainError)?;

            entries.push(entry);
        }

        let outcome = self
            .repository
            .save_job_if_within_quota(&job, &entries, user_id, MAX_JOBS_PER_USER_PER_DAY)
            .await
            .map_err(SubmitProxyCheckJobError::RepoError)?;

        match outcome {
            QuotaCheckOutcome::QuotaExceeded => {
                return Err(SubmitProxyCheckJobError::QuotaExceeded {
                    limit: MAX_JOBS_PER_USER_PER_DAY,
                });
            }
            QuotaCheckOutcome::Saved => {}
        }

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::ProxyCheckJobCreated,
            target_id: job.id().0.to_string(),
            metadata: json!({
                "total_count": job.total_count(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(SubmitProxyCheckJobError::RepoError)?;

        Ok(job)
    }
}
