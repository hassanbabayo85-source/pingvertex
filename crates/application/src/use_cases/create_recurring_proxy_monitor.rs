use std::{error::Error, fmt, sync::Arc};

use crate::{
    dto::{CreateRecurringProxyMonitorRequest, RecurringProxyMonitorResponse},
    errors::PlanLimitError,
    ports::{EntitlementResolver, RecurringProxyMonitorRepository},
};
use domain::entities::{
    MAX_RECURRING_PROXY_MONITORS, MIN_RECURRING_PROXY_INTERVAL_SECONDS,
};
use domain::entities::RecurringProxyMonitor;
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::value_objects::{ProxyCredentials, ProxyHost, is_allowed_proxy_port};
use uuid::Uuid;

use crate::ports::monitor_repository::QuotaCheckOutcome;

type BoxError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub enum CreateRecurringProxyMonitorError {
    Forbidden,
    PlanLimitExceeded(PlanLimitError),
    InvalidHost(String),
    InvalidCredentials(String),
    Other(BoxError),
}

impl fmt::Display for CreateRecurringProxyMonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "forbidden: you cannot create recurring proxy monitors"),
            Self::PlanLimitExceeded(e) => write!(f, "{e}"),
            Self::InvalidHost(m) => write!(f, "invalid proxy host: {m}"),
            Self::InvalidCredentials(m) => write!(f, "invalid proxy credentials: {m}"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl Error for CreateRecurringProxyMonitorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlanLimitExceeded(e) => Some(e),
            Self::Other(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct CreateRecurringProxyMonitorUseCase<R> {
    repository: R,
    entitlement_resolver: Arc<dyn EntitlementResolver>,
}

impl<R> CreateRecurringProxyMonitorUseCase<R>
where
    R: RecurringProxyMonitorRepository,
{
    pub fn new(repository: R, entitlement_resolver: Arc<dyn EntitlementResolver>) -> Self {
        Self {
            repository,
            entitlement_resolver,
        }
    }

    pub async fn execute(
        &self,
        request: CreateRecurringProxyMonitorRequest,
        authenticated_user: AuthenticatedUser,
    ) -> Result<RecurringProxyMonitorResponse, CreateRecurringProxyMonitorError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateRecurringProxyMonitorError::Forbidden);
        }

        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let user_id = authenticated_user.user_id();

        let entitlements = self
            .entitlement_resolver
            .resolve(tenant_id)
            .await
            .map_err(CreateRecurringProxyMonitorError::Other)?;

        let min_interval = entitlements.min_recurring_proxy_interval_seconds;
        if request.interval_seconds < min_interval {
            return Err(CreateRecurringProxyMonitorError::PlanLimitExceeded(
                PlanLimitError::new(
                    MIN_RECURRING_PROXY_INTERVAL_SECONDS,
                    u64::from(request.interval_seconds),
                    u64::from(min_interval),
                    "upgrade_plan",
                ),
            ));
        }

        let host = ProxyHost::new(request.host.clone()).map_err(|e| {
            CreateRecurringProxyMonitorError::InvalidHost(e.to_string())
        })?;

        if !is_allowed_proxy_port(request.port) {
            return Err(CreateRecurringProxyMonitorError::InvalidHost(format!(
                "port {} is not allowed for a proxy monitor",
                request.port
            )));
        }

        let credentials = match (request.username.as_deref(), request.password.as_deref()) {
            (None, None) => None,
            (u, p) => {
                let creds = ProxyCredentials::new(u.unwrap_or(""), p.unwrap_or(""))
                    .map_err(|e| CreateRecurringProxyMonitorError::InvalidCredentials(e.to_string()))?;
                Some(creds)
            }
        };

        let monitor = RecurringProxyMonitor::new(
            Uuid::new_v4(),
            tenant_id,
            user_id,
            request.name,
            host,
            request.port,
            request.scheme,
            credentials,
            request.interval_seconds,
        )
        .map_err(|e| CreateRecurringProxyMonitorError::Other(Box::new(e)))?;

        match self
            .repository
            .save_monitor_if_within_quota(
                &monitor,
                tenant_id,
                entitlements.max_recurring_proxy_monitors,
            )
            .await
            .map_err(CreateRecurringProxyMonitorError::Other)?
        {
            QuotaCheckOutcome::Saved => {}
            QuotaCheckOutcome::QuotaExceeded => {
                return Err(CreateRecurringProxyMonitorError::PlanLimitExceeded(
                    PlanLimitError::new(
                        MAX_RECURRING_PROXY_MONITORS,
                        u64::from(entitlements.max_recurring_proxy_monitors),
                        u64::from(entitlements.max_recurring_proxy_monitors),
                        "upgrade_plan",
                    ),
                ));
            }
        }

        Ok(RecurringProxyMonitorResponse::from_entity(&monitor))
    }
}
