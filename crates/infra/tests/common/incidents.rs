use domain::Incident;
use domain::shared::ids::{IncidentId, MonitorId};
use uuid::Uuid;

pub fn sample_incident(monitor_id: MonitorId) -> Incident {
    Incident::new(
        IncidentId(Uuid::new_v4()),
        monitor_id,
    )
}
