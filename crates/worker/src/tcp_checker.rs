use std::time::{Duration, Instant};

use domain::{CheckErrorKind, CheckResult, Monitor, RegionCode};
use shared::ssrf::resolve_and_validate_host_port;
use tokio::net::TcpStream;
use tokio::time::timeout;
use url::Url;

type Result = std::result::Result<CheckResult, Box<dyn std::error::Error + Send + Sync>>;
type OutcomeResult = std::result::Result<TcpCheckOutcome, Box<dyn std::error::Error + Send + Sync>>;

/// The result of a TCP check execution plus its canonical error
/// classification. Mirrors `HttpCheckOutcome` so active check executors
/// can persist the exact same `ErrorType` taxonomy.
#[derive(Debug, Clone)]
pub struct TcpCheckOutcome {
    pub result: CheckResult,
    pub error_kind: CheckErrorKind,
}

impl TcpCheckOutcome {
    fn success(region: RegionCode, latency_ms: u128) -> Self {
        Self {
            result: CheckResult::success(region, 0, latency_ms),
            error_kind: CheckErrorKind::Unknown,
        }
    }

    fn failure(
        region: RegionCode,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self {
            result: CheckResult::failure(region, latency_ms, error),
            error_kind,
        }
    }
}

/// Parses a host from `monitor.url`. TCP/DNS monitors store the target in
/// `monitor.url` as a full URL (e.g. `http://example.com`), but the SSRF
/// helpers and TCP/DNS checkers need a bare host. If `monitor.url` is
/// already a bare host, it is returned unchanged.
fn extract_host(raw: &str) -> String {
    if raw.contains("://") {
        if let Ok(parsed) = Url::parse(raw) {
            if let Some(host) = parsed.host_str() {
                return host.to_string();
            }
        }
    }
    raw.to_string()
}

/// Performs a TCP connectivity check for a `MonitorKind::Tcp` monitor.
///
/// SECURITY: this is NOT a general-purpose port scanner. The destination
/// host/port is resolved and validated through the SAME SSRF guard every
/// other checker in this codebase uses (`shared::ssrf`), and the connection
/// is pinned to the validated, already-resolved `SocketAddr` — a second DNS
/// lookup at connect time (which could resolve differently, e.g. DNS
/// rebinding) never happens.
pub async fn check_tcp_outcome(monitor: &Monitor) -> OutcomeResult {
    let port = monitor
        .tcp_port
        .ok_or("tcp_port is missing for a Tcp monitor")?;
    let host = extract_host(&monitor.url);

    let started = Instant::now();

    let socket_addr = match resolve_and_validate_host_port(&host, port).await {
        Ok(addr) => addr,
        Err(err) => {
            return Ok(TcpCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::DnsError,
                err,
            ));
        }
    };

    let timeout_duration = Duration::from_secs(monitor.timeout_seconds.into());
    let connect_result = timeout(timeout_duration, TcpStream::connect(socket_addr)).await;

    let latency_ms = started.elapsed().as_millis();

    match connect_result {
        Ok(Ok(_stream)) => Ok(TcpCheckOutcome::success(
            RegionCode::default_region(),
            latency_ms,
        )),
        Ok(Err(err)) => Ok(TcpCheckOutcome::failure(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::ConnectionError,
            format!("TCP connection failed: {err}"),
        )),
        Err(_) => Ok(TcpCheckOutcome::failure(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::Timeout,
            "TCP connection timed out",
        )),
    }
}

/// Backwards-compatible TCP entry point used by the legacy sweep
/// scheduler (`scheduler.rs`).
pub async fn check_tcp(monitor: &Monitor) -> Result {
    let outcome = check_tcp_outcome(monitor).await?;
    Ok(outcome.result)
}

/// Direct TCP connect helper (SSRF validation and DNS pinning have
/// already been performed by the caller). Public only for tests.
#[doc(hidden)]
pub async fn connect_tcp(
    socket_addr: std::net::SocketAddr,
    timeout_duration: Duration,
) -> std::result::Result<(), std::io::Error> {
    match timeout(timeout_duration, TcpStream::connect(socket_addr)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "TCP connection timed out",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::shared::ids::{MonitorId, TenantId, UserId};
    use domain::{
        CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorKind, MonitorName,
        Timeout, Url,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::net::TcpListener;
    use uuid::Uuid;

    fn build_tcp_monitor(url: &str, port: u16) -> Monitor {
        MonitorBuilder::new(
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId::new(),
        )
        .name(MonitorName::new("TCP Test").unwrap())
        .url(Url::new(format!("http://{url}")).unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(2).unwrap())
        .kind(MonitorKind::Tcp)
        .tcp_port(port)
        .try_build()
        .unwrap()
    }

    #[tokio::test]
    async fn rejects_private_ip_target() {
        let result = shared::ssrf::resolve_and_validate_host_port("10.0.0.5", 5432).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn missing_tcp_port_is_an_error() {
        let mut monitor = build_tcp_monitor("example.com", 443);
        monitor.tcp_port = None;
        let result = check_tcp_outcome(&monitor).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn successful_connection_returns_up() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();

        tokio::spawn(async move { if let Ok((_stream, _)) = listener.accept().await {} });

        let result = connect_tcp(addr, Duration::from_secs(2)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn refused_connection_returns_error() {
        // Bind a listener to obtain a free port, then drop it so the port
        // is closed and the next connect attempt should be refused.
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);

        let result = connect_tcp(addr, Duration::from_secs(2)).await;
        assert!(result.is_err());
    }
}
