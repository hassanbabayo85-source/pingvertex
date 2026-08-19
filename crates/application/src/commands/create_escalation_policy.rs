use domain::EscalationStep;
use domain::shared::ids::MonitorId;

/// Command to create an escalation policy.
///
/// Idan `monitor_id` is `Some(...)`, policy ɗin yana aiki ga monitor ɗaya.
/// Idan `None`, yana aiki ga dukkan monitors na wannan user.
#[derive(Debug, Clone)]
pub struct CreateEscalationPolicy {
    pub monitor_id: Option<MonitorId>,
    pub steps: Vec<EscalationStep>,
}
