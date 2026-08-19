use domain::shared::ids::MonitorId;

#[derive(Debug, Clone, Copy)]
pub struct DeleteMonitor {
    pub id: MonitorId,
}
