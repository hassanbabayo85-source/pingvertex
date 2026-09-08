use crate::MonitorKind;

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
