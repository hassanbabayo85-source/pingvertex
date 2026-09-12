use domain::shared::ids::IncidentId;

/// Command to acknowledge an incident.
#[derive(Debug, Clone, Copy)]
pub struct AcknowledgeIncident {
    pub incident_id: IncidentId,
}
