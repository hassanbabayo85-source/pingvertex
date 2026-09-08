use crate::MonitorKind;
use std::fmt;
use std::str::FromStr;

/// Free PAYG entitlement constants. The active database pricing configuration
/// is the runtime source of truth; these values are the safe bootstrap/test
/// defaults and must stay aligned with the product contract.
pub const FREE_MONITOR_LIMIT: u64 = 10;
pub const FREE_MIN_INTERVAL_SECONDS: u32 = 300;
/// Minimum product-defined wallet top-up: $5 in micro-USD.
pub const MIN_PAYG_TOP_UP_UNITS: i64 = 5_000_000;

/// Canonical server-owned PAYG operation identifiers. Runtime code derives
/// these values; clients never submit prices or operation identifiers.
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
    Storage,
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
            Self::Storage => "storage",
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
        Self::Storage,
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
        assert_eq!(PaygOperation::ALL.len(), 15);
        assert!(PaygOperation::is_canonical("http_check.one_time"));
        assert!(PaygOperation::is_canonical("http_check.monitoring"));
        assert!(PaygOperation::is_canonical("heartbeat_processing"));
        assert!(PaygOperation::is_canonical("network_egress"));
        assert!(!PaygOperation::is_canonical("monitor_check"));
        assert!(!PaygOperation::is_canonical("heartbeat_ping"));
        assert!(!PaygOperation::is_canonical("proxy_check"));
    }

    #[test]
    fn free_and_topup_contract_constants_are_stable() {
        assert_eq!(FREE_MONITOR_LIMIT, 10);
        assert_eq!(FREE_MIN_INTERVAL_SECONDS, 300);
        assert_eq!(MIN_PAYG_TOP_UP_UNITS, 5_000_000);
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
}
