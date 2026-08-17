use std::{error::Error, fmt, sync::Arc};

use domain::MaintenanceWindow;

use crate::{
    commands::CreateMaintenanceWindow,
    ports::{IdGenerator, MaintenanceWindowRepository, MonitorRepository},
};

/// Error returned by `CreateMaintenanceWindowUseCase`.
#[derive(Debug)]
pub enum CreateMaintenanceWindowError {
    /// Monitor doesn't exist, or exists but isn't owned by this user.
    MonitorNotFound,
    /// Domain validation failed (bad dates, too long reason, duration > 7 days).
    InvalidWindow(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateMaintenanceWindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MonitorNotFound => write!(f, "monitor not found or access denied"),
            Self::InvalidWindow(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateMaintenanceWindowError {}

/// Creates a maintenance window on a monitor, verifying ownership.
pub struct CreateMaintenanceWindowUseCase {
    monitor_repository: Arc<dyn MonitorRepository>,
    maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    id_generator: Arc<dyn IdGenerator>,
}

impl CreateMaintenanceWindowUseCase {
    pub fn new(
        monitor_repository: Arc<dyn MonitorRepository>,
        maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            monitor_repository,
            maintenance_window_repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        command: CreateMaintenanceWindow,
    ) -> Result<MaintenanceWindow, CreateMaintenanceWindowError> {
        self.monitor_repository
            .find_by_id_for_user(command.monitor_id, command.user_id)
            .await
            .map_err(CreateMaintenanceWindowError::Other)?
            .ok_or(CreateMaintenanceWindowError::MonitorNotFound)?;

        let id = self.id_generator.next_maintenance_window_id().await;

        let window = MaintenanceWindow::new(
            id,
            command.monitor_id,
            command.starts_at,
            command.ends_at,
            command.reason,
        )
        .map_err(|err| CreateMaintenanceWindowError::InvalidWindow(err.to_string()))?;

        self.maintenance_window_repository
            .create(&window)
            .await
            .map_err(CreateMaintenanceWindowError::Other)?;

        Ok(window)
    }
}
