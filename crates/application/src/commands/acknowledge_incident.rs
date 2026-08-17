use domain::shared::ids::{IncidentId, UserId};

/// Command to acknowledge an incident.
#[derive(Debug, Clone, Copy)]
pub struct AcknowledgeIncident {
    pub incident_id: IncidentId,
    pub user_id: UserId,
}
