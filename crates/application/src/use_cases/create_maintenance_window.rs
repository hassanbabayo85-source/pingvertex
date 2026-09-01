use std::{error::Error, fmt, sync::Arc};

use chrono::Utc;
use serde_json::json;

use domain::{
    AuditAction, AuditLogEntry, AuditLogEntryId, MaintenanceWindow,
    shared::policies::{AuthorizationPolicy, TenantOwned},
    shared::types::{AuthenticatedUser, Permission},
};

use crate::{
    commands::CreateMaintenanceWindow,
    ports::{AuditLogEntryRepository, IdGenerator, MaintenanceWindowRepository, MonitorRepository},
};

#[derive(Debug)]
pub enum CreateMaintenanceWindowError {
    Forbidden,
    MonitorNotFound,
    InvalidWindow(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMaintenanceWindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(
                f,
                "forbidden: you do not have permission to create maintenance windows"
            ),
            Self::MonitorNotFound => write!(f, "monitor not found or access denied"),
            Self::InvalidWindow(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMaintenanceWindowError {}

pub struct CreateMaintenanceWindowUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    id_generator: Arc<dyn IdGenerator>,
    audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
}

impl CreateMaintenanceWindowUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
        id_generator: Arc<dyn IdGenerator>,
        audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    ) -> Self {
        Self {
            monitor_repository,
            maintenance_window_repository,
            id_generator,
            audit_log_entry_repository,
        }
    }

    pub async fn execute(
        &self,
        command: CreateMaintenanceWindow,
        authenticated_user: AuthenticatedUser,
    ) -> Result<MaintenanceWindow, CreateMaintenanceWindowError> {
        if !authenticated_user.can(Permission::CreateMonitor) {
            return Err(CreateMaintenanceWindowError::Forbidden);
        }

        let user_id = authenticated_user.user_id();
        let tenant_id = authenticated_user.tenant_context().tenant_id();

        let monitor = self
            .monitor_repository
            .find_by_id(command.monitor_id)
            .await
            .map_err(CreateMaintenanceWindowError::Other)?
            .ok_or(CreateMaintenanceWindowError::MonitorNotFound)?;

        if !AuthorizationPolicy::same_tenant(
            &authenticated_user.tenant_context(),
            monitor.tenant_id(),
        ) {
            return Err(CreateMaintenanceWindowError::MonitorNotFound);
        }

        let id = self.id_generator.next_maintenance_window_id().await;

        let window = MaintenanceWindow::new(
            id,
            command.monitor_id,
            tenant_id,
            command.starts_at,
            command.ends_at,
            command.reason,
        )
        .map_err(|err| CreateMaintenanceWindowError::InvalidWindow(err.to_string()))?;

        self.maintenance_window_repository
            .create(&window)
            .await
            .map_err(CreateMaintenanceWindowError::Other)?;

        let entry = AuditLogEntry {
            id: AuditLogEntryId::new(),
            actor_user_id: user_id,
            tenant_id: Some(tenant_id),
            action: AuditAction::MaintenanceWindowCreated,
            target_id: window.id.0.to_string(),
            metadata: json!({
                "monitor_id": window.monitor_id.0.to_string(),
            }),
            created_at: Utc::now(),
        };
        self.audit_log_entry_repository
            .create(&entry)
            .await
            .map_err(CreateMaintenanceWindowError::Other)?;

        Ok(window)
    }
}
