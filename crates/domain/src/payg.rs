use crate::MonitorKind;
use std::fmt;
use std::str::FromStr;

/// Minimum product-defined wallet top-up: $5 in nano-USD.
///
/// GAP 3 — the PAYG unit is nano-USD (1 unit = $10^-9 USD). $5 therefore
/// equals 5_000_000_000 units.
pub const MIN_PAYG_TOP_UP_UNITS: i64 = 5_000_000_000;

/// PAYG network egress is billed in binary megabytes to match the existing
/// bounded response-body safety limits and storage metering semantics.
pub const PAYG_NETWORK_EGRESS_UNIT_BYTES: u64 = 1024 * 1024;

/// Non-canonical historical operations that must never be billed at
/// runtime. This list is a Rust-side mirror of the row set in
/// `payg_retired_operations`, kept here so tests can assert the
/// retirement set is explicit and so new code never accidentally
/// re-introduces one of these strings.
pub const RETIRED_PAYG_OPERATIONS: &[&str] = &[
    "proxy_check",
    "monitor_check",
    "SMS",
    "heartbeat_ping",
    "storage",
];

/// Canonical server-owned PAYG operation identifiers. Runtime code derives
/// these values; clients never submit prices or operation identifiers.
///
/// One-time and monitoring variants are always distinct: `*_check.one_time`
/// is billed once per Test Now request (per region), `*_check.monitoring`
/// is billed per scheduled recurring execution.
///
/// **Units are nano-USD** (1 = $10^-9 USD). `storage` was retired in GAP 3
/// and is no longer a canonical operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaygOperation {
    HttpOneTime,
    TcpOneTime,
    DnsOneTime,
    ApiFlowOneTime,
    ProxyOneTime,
    HttpMonitoring,
    TcpMonitoring,
    DnsMonitoring,
    HeartbeatProcessing,
    ApiFlowMonitoring,
    ProxyMonitoring,
    RumEvent,
    Sms,
    NetworkEgress,
}
impl PaygOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HttpOneTime => "http_check.one_time",
            Self::TcpOneTime => "tcp_check.one_time",
            Self::DnsOneTime => "dns_check.one_time",
            Self::ApiFlowOneTime => "api_flow_check.one_time",
            Self::ProxyOneTime => "proxy_check.one_time",
            Self::HttpMonitoring => "http_check.monitoring",
            Self::TcpMonitoring => "tcp_check.monitoring",
            Self::DnsMonitoring => "dns_check.monitoring",
            Self::HeartbeatProcessing => "heartbeat_processing",
            Self::ApiFlowMonitoring => "api_flow_check.monitoring",
            Self::ProxyMonitoring => "proxy_check.monitoring",
            Self::RumEvent => "rum_event",
            Self::Sms => "sms",
            Self::NetworkEgress => "network_egress",
        }
    }
}

impl PaygOperation {
    pub const ALL: &[PaygOperation] = &[
        Self::HttpOneTime,
        Self::TcpOneTime,
        Self::DnsOneTime,
        Self::ApiFlowOneTime,
        Self::ProxyOneTime,
        Self::HttpMonitoring,
        Self::TcpMonitoring,
        Self::DnsMonitoring,
        Self::HeartbeatProcessing,
        Self::ApiFlowMonitoring,
        Self::ProxyMonitoring,
        Self::RumEvent,
        Self::Sms,
        Self::NetworkEgress,
    ];

    pub fn is_canonical(operation: &str) -> bool {
        Self::ALL
            .iter()
            .any(|candidate| candidate.as_str() == operation)
    }
}

impl fmt::Display for PaygOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PaygOperation {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|op| op.as_str() == value)
            .ok_or("unknown PAYG operation")
    }
}

pub const fn monitor_operation(kind: MonitorKind, one_time: bool) -> PaygOperation {
    match (kind, one_time) {
        (MonitorKind::Http, true) => PaygOperation::HttpOneTime,
        (MonitorKind::Tcp, true) => PaygOperation::TcpOneTime,
        (MonitorKind::Dns, true) => PaygOperation::DnsOneTime,
        (MonitorKind::Flow, true) => PaygOperation::ApiFlowOneTime,
        (MonitorKind::Http, false) => PaygOperation::HttpMonitoring,
        (MonitorKind::Tcp, false) => PaygOperation::TcpMonitoring,
        (MonitorKind::Dns, false) => PaygOperation::DnsMonitoring,
        (MonitorKind::Flow, false) => PaygOperation::ApiFlowMonitoring,
        (MonitorKind::Heartbeat, _) => PaygOperation::HeartbeatProcessing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_operation_set_matches_part1_price_book() {
        assert_eq!(PaygOperation::ALL.len(), 14);
        assert!(PaygOperation::is_canonical("http_check.one_time"));
        assert!(PaygOperation::is_canonical("http_check.monitoring"));
        assert!(PaygOperation::is_canonical("proxy_check.one_time"));
        assert!(PaygOperation::is_canonical("proxy_check.monitoring"));
        assert!(PaygOperation::is_canonical("heartbeat_processing"));
        assert!(PaygOperation::is_canonical("network_egress"));
        assert!(!PaygOperation::is_canonical("monitor_check"));
        assert!(!PaygOperation::is_canonical("heartbeat_ping"));
        assert!(!PaygOperation::is_canonical("proxy_check"));
        assert!(!PaygOperation::is_canonical("storage"));
    }

    #[test]
    fn topup_contract_constant_is_stable() {
        // $5 in nano-USD (1 nano-USD = 10^-9 USD).
        assert_eq!(MIN_PAYG_TOP_UP_UNITS, 5_000_000_000);
    }

    #[test]
    fn operations_are_distinct() {
        assert_eq!(
            monitor_operation(MonitorKind::Http, false).as_str(),
            "http_check.monitoring"
        );
        assert_eq!(
            monitor_operation(MonitorKind::Http, true).as_str(),
            "http_check.one_time"
        );
        assert_ne!(
            monitor_operation(MonitorKind::Http, false).as_str(),
            monitor_operation(MonitorKind::Http, true).as_str()
        );
    }

    #[test]
    fn proxy_one_time_and_monitoring_are_distinct() {
        assert_ne!(
            PaygOperation::ProxyOneTime.as_str(),
            PaygOperation::ProxyMonitoring.as_str()
        );
    }

    #[test]
    fn storage_is_retired_not_canonical() {
        assert!(!PaygOperation::is_canonical("storage"));
        assert!(RETIRED_PAYG_OPERATIONS.contains(&"storage"));
    }
}
