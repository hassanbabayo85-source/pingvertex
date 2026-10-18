use domain::EscalationStep;
use domain::shared::ids::MonitorId;

/// Command to create an escalation policy.
///
/// If `monitor_id` is `Some(...)`, the policy applies to that single monitor.
/// If `None`, it applies to all of the user's monitors.
#[derive(Debug, Clone)]
pub struct CreateEscalationPolicy {
    pub monitor_id: Option<MonitorId>,
    pub steps: Vec<EscalationStep>,
}
