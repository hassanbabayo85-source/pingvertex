use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::policies::TenantOwned;
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{AuditAction, AuditLogEntry, AuditLogEntryId, Incident};
use serde_json::json;

use crate::{
    commands::AcknowledgeIncident,
    ports::{AuditLogEntryRepository, IncidentRepository, MonitorRepository},
};

/// Error returned by `AcknowledgeIncidentUseCase`.
#[derive(Debug)]
pub enum AcknowledgeIncidentError {
    Forbidden,
    /// Incident doesn't exist, or exists but belongs to a monitor the user
    /// doesn't own. Deliberately the same outward result either way.
    NotFound,
    /// Incident is already resolved — acknowledgement only makes sense for
    /// open incidents.
    AlreadyResolved,
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for AcknowledgeIncidentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to acknowledge incidents"
            ),
            Self::NotFound => write!(f, "incident not found or access denied"),
            Self::AlreadyResolved => write!(f, "incident is already resolved"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for AcknowledgeIncidentError {}

/// Acknowledges an incident, verifying that the user owns the monitor
/// associated with that incident.
pub struct AcknowledgeIncidentUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    incident_repository: Arc<dyn IncidentRepository>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl AcknowledgeIncidentUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        incident_repository: Arc<dyn IncidentRepository>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            incident_repository,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: AcknowledgeIncident,
        authenticated_user: AuthenticatedUser,
    ) -> Result<Incident, AcknowledgeIncidentError> {
        if !authenticated_user.can(Permission::AcknowledgeIncident) {
            return Err(AcknowledgeIncidentError::Forbidden);
        }

        self.execute_for_tenant(
            command,
            authenticated_user.tenant_context().tenant_id(),
            authenticated_user.user_id(),
        )
        .await
    }

    /// Tenant-scoped variant of `execute` that takes `tenant_id` /
    /// `acting_user_id` directly instead of an `AuthenticatedUser`.
    ///
    /// Added for the API-key-authenticated public surface
    /// (`POST /api/v1/external/incidents/{id}/acknowledge`). The
    /// audit log records `acting_user_id` (the API key's
    /// `ApiKeyPrincipal::user_id`, i.e. the key's creator) exactly like
    /// `CreateApiKeyUseCase` treats that field: an audit trail, not an
    /// access-control gate. `execute` above is now a thin wrapper
    /// around this.
    pub async fn execute_for_tenant(
        &self,
        command: AcknowledgeIncident,
        tenant_id: TenantId,
        acting_user_id: UserId,
    ) -> Result<Incident, AcknowledgeIncidentError> {
        let mut incident = self
            .incident_repository
            .find_by_id(command.incident_id)
            .await
            .map_err(AcknowledgeIncidentError::Other)?
            .ok_or(AcknowledgeIncidentError::NotFound)?;

        let monitor = self
            .monitor_repository
            .find_by_id(incident.monitor_id)
            .await
            .map_err(AcknowledgeIncidentError::Other)?
            .ok_or(AcknowledgeIncidentError::NotFound)?;

        if monitor.tenant_id() != tenant_id {
            return Err(AcknowledgeIncidentError::NotFound);
        }

        if incident.resolved_at.is_some() {
            return Err(AcknowledgeIncidentError::AlreadyResolved);
        }

        let now = Utc::now();
        incident.acknowledge(acting_user_id, now);

        self.incident_repository
            .update(&incident)
            .await
            .map_err(AcknowledgeIncidentError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: acting_user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::IncidentAcknowledged,
            target_id: incident.id.0.to_string(),
            metadata: json!({
                "monitor_id": incident.monitor_id.0.to_string(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(AcknowledgeIncidentError::Other)?;

        Ok(incident)
    }
}
