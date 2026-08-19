use serde::{Deserialize, Serialize};

/// The kind of check a monitor performs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorKind {
    #[default]
    Http,
    Tcp,
    Dns,
    Heartbeat,
}
