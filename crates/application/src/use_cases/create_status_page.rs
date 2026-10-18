use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use crate::{
    dto::CreateStatusPageRequest,
    errors::PlanLimitError,
    ports::{
        AuditLogEntryRepository, EntitlementResolver, IdGenerator, MonitorRepository,
        StatusPageRepository,
    },
};
use domain::shared::policies::{AuthorizationPolicy, TenantOwned};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, MonitorId, Subdomain,
    entities::{CreateStatusPage, StatusPage},
    entities::MAX_STATUS_PAGES,
};

use crate::ports::monitor_repository::QuotaCheckOutcome;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

// NOTE (TASK 4A): The old hardcoded `MAX_STATUS_PAGES_PER_USER = 3`
// constant has been REMOVED. The effective limit for a tenant is now
// resolved per-request from `plan_entitlements.max_status_pages`
// via `EntitlementResolver`. Existing tenants' pages are not deleted or
// transferred; only new creation is limited.

#[derive(Debug)]
pub enum CreateStatusPageError {
    Forbidden,
    PlanLimitExceeded(PlanLimitError),
    SubdomainTaken { subdomain: String },
    MonitorNotFound { monitor_id: MonitorId },
    EntitlementResolverMissing,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateStatusPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create status pages"
            ),
            Self::PlanLimitExceeded(e) => write!(f, "{e}"),
            Self::SubdomainTaken { subdomain } => {
                write!(f, "subdomain \"{subdomain}\" is already taken")
            }
            Self::MonitorNotFound { monitor_id } => {
                write!(f, "monitor {monitor_id} not found or access denied")
            }
            Self::EntitlementResolverMissing => write!(
                f,
                "entitlement resolver is not configured; cannot enforce plan limits"
            ),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateStatusPageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlanLimitExceeded(e) => Some(e),
            Self::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

pub struct CreateStatusPageUseCase<R, G, M> {
    repository: R,
    id_generator: G,
    monitor_repository: M,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    entitlement_resolver: Option<Arc<dyn EntitlementResolver>>,
}

impl<R, G, M> CreateStatusPageUseCase<R, G, M>
where
    R: StatusPageRepository,
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
            entitlement_resolver: None,
        }
    }

    /// TASK 4A — installs the authoritative `EntitlementResolver`.
    /// Production handlers MUST call this. If absent, `execute` fails
    /// closed with `EntitlementResolverMissing`.
    pub fn with_entitlement_resolver(
        mut self,
        resolver: Arc<dyn EntitlementResolver>,
    ) -> Self {
        self.entitlement_resolver = Some(resolver);
        self
    }

    pub async fn execute(
        &self,
        request: CreateStatusPageRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<StatusPage, CreateStatusPageError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateStatusPageError::Forbidden);
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        // ── Resolve entitlements (fail closed) ──────────────────
        let resolver = self
            .entitlement_resolver
            .as_ref()
            .ok_or(CreateStatusPageError::EntitlementResolverMissing)?;
        let entitlements = resolver
            .resolve(tenant_id)
            .await
            .map_err(CreateStatusPageError::Other)?;

        // ── Domain validation ───────────────────────────────────
        let subdomain = Subdomain::new(request.subdomain.clone())
            .map_err(|err| CreateStatusPageError::Other(Box::new(err)))?;

        if self
            .repository
            .find_by_subdomain(subdomain.as_str())
            .await
            .map_err(CreateStatusPageError::Other)?
            .is_some()
        {
            return Err(CreateStatusPageError::SubdomainTaken {
                subdomain: subdomain.as_str().to_owned(),
            });
        }

        self.assert_monitors_owned(&authenticated_user, &request.monitor_ids)
            .await?;

        // ── Build the entity (do NOT save yet) ──────────────────
        let status_page = self
            .build_status_page(request, subdomain, user_id, tenant_id)
            .await
            .map_err(CreateStatusPageError::Other)?;

        // ── Atomic tenant-scoped quota check + save ─────────────
        match self
            .repository
            .save_if_within_quota(&status_page, tenant_id, entitlements.max_status_pages)
            .await
            .map_err(CreateStatusPageError::Other)?
        {
            QuotaCheckOutcome::Saved => {}
            QuotaCheckOutcome::QuotaExceeded => {
                return Err(CreateStatusPageError::PlanLimitExceeded(PlanLimitError::new(
                    MAX_STATUS_PAGES,
                    u64::from(entitlements.max_status_pages),
                    u64::from(entitlements.max_status_pages),
                    "upgrade_plan",
                )));
            }
        }

        // ── Audit log (only after the save succeeded) ───────────
        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::StatusPageCreated,
            target_id: status_page.id.0.to_string(),
            metadata: json!({
                "subdomain": status_page.subdomain.as_str(),
                "monitor_count": status_page.monitor_ids.len(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(CreateStatusPageError::Other)?;

        Ok(status_page)
    }

    async fn assert_monitors_owned(
        &self,
        authenticated_user: &AuthenticatedUser,
        monitor_ids: &[MonitorId],
    ) -> Result<(), CreateStatusPageError> {
        for &monitor_id in monitor_ids {
            let monitor = self
                .monitor_repository
                .find_by_id(monitor_id)
                .await
                .map_err(CreateStatusPageError::Other)?
                .ok_or(CreateStatusPageError::MonitorNotFound { monitor_id })?;

            if !AuthorizationPolicy::same_tenant(
                &authenticated_user.tenant_context(),
                monitor.tenant_id(),
            ) {
                return Err(CreateStatusPageError::MonitorNotFound { monitor_id });
            }
        }
        Ok(())
    }

    async fn build_status_page(
        &self,
        request: CreateStatusPageRequest,
        subdomain: Subdomain,
        user_id: domain::UserId,
        tenant_id: domain::TenantId,
    ) -> UseCaseResult<StatusPage> {
        let id = self.id_generator.next_status_page_id().await;
        let status_page = StatusPage::create(CreateStatusPage {
            id,
            user_id,
            tenant_id,
            name: request.name,
            subdomain,
            monitor_ids: request.monitor_ids,
        })?;
        Ok(status_page)
    }
}
