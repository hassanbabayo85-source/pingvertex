//! Metrics label helper: maps a monitor kind + one-time flag to a
//! stable Prometheus label string. Phase 1C-1c-1b-fix-2.
//!
//! Restored from `payg.rs` after the billing module was deleted. Only
//! the label strings are kept; the `PaygOperation` enum and its
//! billing semantics are gone.

use crate::MonitorKind;

pub const fn monitor_operation(kind: MonitorKind, one_time: bool) -> &'static str {
    match (kind, one_time) {
        (MonitorKind::Http, true) => "http_check.one_time",
        (MonitorKind::Tcp, true) => "tcp_check.one_time",
        (MonitorKind::Dns, true) => "dns_check.one_time",
        (MonitorKind::Flow, true) => "api_flow_check.one_time",
        (MonitorKind::Http, false) => "http_check.monitoring",
        (MonitorKind::Tcp, false) => "tcp_check.monitoring",
        (MonitorKind::Dns, false) => "dns_check.monitoring",
        (MonitorKind::Flow, false) => "api_flow_check.monitoring",
        (MonitorKind::Heartbeat, _) => "heartbeat_processing",
    }
}
