use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    commands::RequestCustomDomain,
    ports::{AuditLogEntryRepository, StatusPageRepository},
};
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::AuthenticatedUser;
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, CustomDomain, entities::StatusPage};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Errors from [`RequestCustomDomainUseCase::execute`] that need distinct
/// HTTP status mapping at the API layer (same pattern as
/// `CreateStatusPageError` for status page creation).
#[derive(Debug)]
pub enum RequestCustomDomainError {
    /// The requested domain is already `Verified` on a DIFFERENT status
    /// page. Deliberately does NOT block a `Pending` claim by another
    /// tenant on the same domain string — see the SECURITY/rationale
    /// notes on `idx_status_pages_verified_custom_domain` in migration
    /// 0018 and on `StatusPageRepository::find_by_verified_custom_domain`:
    /// only DNS ownership (proven via the TXT-record challenge) can ever
    /// actually verify a domain, so an unverified duplicate claim is
    /// harmless and not squatting-exploitable. This variant only fires
    /// once someone has *already* proven ownership.
    AlreadyClaimed {
        domain: String,
    },
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for RequestCustomDomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyClaimed { domain } => write!(
                f,
                "custom domain \"{domain}\" is already verified for another status page"
            ),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for RequestCustomDomainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AlreadyClaimed { .. } => None,
            Self::Other(err) => Some(err.as_ref()),
        }
    }
}

pub struct RequestCustomDomainUseCase<R> {
    repository: R,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl<R> RequestCustomDomainUseCase<R>
where
    R: StatusPageRepository,
{
    pub fn new(
        repository: R,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: RequestCustomDomain,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Option<StatusPage>, RequestCustomDomainError> {
        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let mut status_page = match self
            .repository
            .find_by_id(command.id)
            .await
            .map_err(RequestCustomDomainError::Other)?
        {
            Some(status_page) => status_page,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            status_page.tenant_id(),
        ) {
            return Ok(None);
        }

        let domain = CustomDomain::new(command.domain)
            .map_err(|err| RequestCustomDomainError::Other(Box::new(err)))?;

        // Tenant isolation: reject outright if this domain is already
        // VERIFIED on a different status page. A `Pending` claim by
        // another tenant on the same domain string is deliberately left
        // alone — see `RequestCustomDomainError::AlreadyClaimed` above.
        if let Some(existing) = self
            .repository
            .find_by_verified_custom_domain(domain.as_str())
            .await
            .map_err(RequestCustomDomainError::Other)?
        {
            if existing.id != status_page.id {
                return Err(RequestCustomDomainError::AlreadyClaimed {
                    domain: domain.as_str().to_owned(),
                });
            }
        }

        status_page.request_custom_domain(domain);

        self.repository
            .update(&status_page)
            .await
            .map_err(RequestCustomDomainError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::CustomDomainRequested,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "subdomain": status_page.subdomain.as_str(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(RequestCustomDomainError::Other)?;

        Ok(Some(status_page))
    }
}
