use serde::{Deserialize, Serialize};
/// The kind of check a monitor performs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorKind {
    #[default]
    #[serde(alias = "http")]
    Http,
    #[serde(alias = "tcp")]
    Tcp,
    #[serde(alias = "dns")]
    Dns,
    #[serde(alias = "heartbeat")]
    Heartbeat,
    /// A multi-step API flow (see `domain::entities::ApiFlow`, keyed by this
    /// monitor's id via `ApiFlow::monitor_id`). The `Monitor` itself carries
    /// no flow-specific fields — all step configuration lives in the
    /// separate `ApiFlow`/`ApiFlowStep` aggregate, fetched at check time by
    /// `worker::FlowCheckExecutor` via `ApiFlowRepository::find_by_monitor_id`.
    #[serde(alias = "flow")]
    Flow,
}
