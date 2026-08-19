use domain::shared::ids::MonitorId;

/// Query for retrieving a monitor by its ID.
///
/// No `user_id` here anymore — tenant-wide sharing means any authenticated
/// member of the monitor's tenant may fetch it. The use case checks that
/// via `AuthorizationPolicy::same_tenant` after loading the monitor.
#[derive(Debug, Clone, Copy)]
pub struct GetMonitor {
    pub id: MonitorId,
}
