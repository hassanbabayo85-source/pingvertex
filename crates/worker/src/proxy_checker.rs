//! Worker-side execution of a `ProxyCheckJob`: checks every `ProxyEntry`
//! belonging to the job concurrently (bounded by a semaphore), persists
//! each entry's result as it finishes, then marks the job completed.
//!
//! SSRF defense: every entry is resolved/validated via
//! `crate::ssrf::resolve_and_validate_destination` (Layer 2 — DNS
//! resolution + private-IP rejection + address pinning) immediately
//! before connecting, exactly as `crate::checker::check_monitor` does for
//! monitor URLs. No new SSRF logic is implemented here; this module only
//! calls the existing shared validator.
//!
//! Protocol verification is manual and pinned to the validated
//! `SocketAddr`:
//! - HTTP proxies are verified with an absolute-form HTTP GET to
//!   `http://example.com/`.
//! - HTTPS proxies are first connected over TLS to the proxy, then
//!   verified with a CONNECT tunnel to `example.com:443` and a TLS
//!   handshake through that tunnel.
//! - SOCKS4/4a proxies are verified with a SOCKS4a CONNECT handshake to
//!   `example.com:443`, then a TLS handshake through the tunnel. SOCKS4a
//!   is used rather than plain SOCKS4 so that the proxy server — not this
//!   worker — resolves `example.com`, which is what a real user of the
//!   proxy would expect. (SOCKS4/4a is IPv4-only by protocol design;
//!   `example.com` has A records, so this works.)
//! - SOCKS5 proxies are verified with a full SOCKS5 handshake to
//!   `example.com:443`, then a TLS handshake through the tunnel.
//!
//! `supports_https` is set only when a TLS handshake through the proxy to
//! `example.com:443` actually succeeds.
//!
//! Response size limits and timeouts bound all proxy-generated data.

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use application::ports::ProxyCheckJobRepository;
use application::{MeterUsageInput, MeterUsageUseCase, MeteredAttempt};
use base64::Engine;
use domain::ProxyScheme;
use domain::entities::{ProxyCheckJob, ProxyEntry};
use domain::enums::{ProxyCheckOutcome, ProxyFailureKind};
use domain::value_objects::{ProxyCredentials, ProxyResult};
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Semaphore,
    task::JoinSet,
    time::timeout,
};
use tracing::{error, info, warn};

/// Upper bound on how many proxy entries within a single job are checked
/// concurrently. Keeps one bulk job from monopolizing this worker
/// process's connection/file-descriptor budget while other monitors,
/// flows, and jobs belonging to other users are also being processed
/// concurrently in the same process.
const MAX_CONCURRENT_CHECKS_PER_JOB: usize = 20;

/// Per-entry timeout for the TCP connect attempt.
const PER_ENTRY_TIMEOUT_SECS: u64 = 10;

/// Per-protocol-step timeout (read/write/TLS handshake) within a proxy
/// check. A malicious/slow proxy must not stall a job indefinitely.
const PROTOCOL_TIMEOUT_SECS: u64 = 5;

/// Maximum number of bytes accepted from a proxy when reading HTTP
/// response/status headers. Anything larger is treated as a protocol
/// failure.
const MAX_PROXY_RESPONSE_HEADER_BYTES: usize = 8192;

/// Fixed public target used to verify HTTPS/CONNECT/SOCKS tunneling.
/// Deliberately constant, never user input.
const HTTPS_VERIFY_TARGET_HOST: &str = "example.com";
const HTTPS_VERIFY_TARGET_PORT: u16 = 443;

fn proxy_scheme_str(scheme: &ProxyScheme) -> &'static str {
    match scheme {
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
        ProxyScheme::Socks4 => "socks4",
        ProxyScheme::Socks5 => "socks5",
    }
}

fn elapsed_ms(started: Instant) -> u32 {
    started.elapsed().as_millis().try_into().unwrap_or(u32::MAX)
}

fn legacy_error_code(scheme: &ProxyScheme, message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("ssrf blocked") {
        return "PRIVATE_IP_BLOCKED";
    }
    if lower.contains("failed to resolve") || lower.contains("did not resolve") {
        return "DNS_FAILED";
    }
    if lower.contains("invalid monitor url")
        || lower.contains("has no host")
        || lower.contains("invalid port")
    {
        return "INVALID_PROXY";
    }
    if lower.contains("connection refused") {
        return "CONNECTION_REFUSED";
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return if lower.contains("target")
            || lower.contains("tls handshake") && lower.contains("target")
        {
            "TARGET_TIMEOUT"
        } else {
            "CONNECTION_TIMEOUT"
        };
    }
    if lower.contains("authentication failed") || lower.contains("auth") && lower.contains("failed")
    {
        return "AUTH_FAILED";
    }
    if lower.contains("tls") {
        return "TLS_FAILED";
    }
    match scheme {
        ProxyScheme::Http => "HTTP_PROXY_FAILED",
        ProxyScheme::Https => "HTTPS_PROXY_FAILED",
        ProxyScheme::Socks4 => "SOCKS4_FAILED",
        ProxyScheme::Socks5 => "SOCKS5_FAILED",
    }
}

/// Aggregated result of checking a single proxy entry.
///
/// This is the intermediate shape between the raw protocol helpers
/// (which only know "did this step succeed or not") and the final
/// `ProxyResult` (which is what the DB stores and the API serves).
/// It exists to answer the two questions PART 3 requires us to keep
/// separate:
///
///   * `proxy_ok`  — did the proxy itself work? (TCP + handshake + auth)
///   * `target_ok` — did the target through the proxy work? (only
///     meaningful if `proxy_ok` is true)
///
/// From those two bits we derive:
///
///   * (true,  true ) -> Working
///   * (true,  false) -> WorkingTargetUnreachable
///   * (false, _    ) -> ProxyFailed
#[derive(Debug, Clone)]
struct CheckAttempt {
    proxy_ok: bool,
    target_ok: bool,
    supports_https: bool,
    connection_latency_ms: Option<u32>,
    tls_handshake_latency_ms: Option<u32>,
    ttfb_ms: Option<u32>,
    http_status_code: Option<u16>,
    failure_kind: Option<ProxyFailureKind>,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl CheckAttempt {
    fn new() -> Self {
        Self {
            proxy_ok: false,
            target_ok: false,
            supports_https: false,
            connection_latency_ms: None,
            tls_handshake_latency_ms: None,
            ttfb_ms: None,
            http_status_code: None,
            failure_kind: None,
            error_code: None,
            error_message: None,
        }
    }

    fn outcome(&self) -> ProxyCheckOutcome {
        match (self.proxy_ok, self.target_ok) {
            (true, true) => ProxyCheckOutcome::Working,
            (true, false) => ProxyCheckOutcome::WorkingTargetUnreachable,
            (false, _) => ProxyCheckOutcome::ProxyFailed,
        }
    }

    /// Mark the whole attempt as successful — proxy and target both worked.
    fn succeed(&mut self) {
        self.proxy_ok = true;
        self.target_ok = true;
    }

    /// Mark a proxy-side failure. Implies the target never got a chance
    /// to be reached.
    fn fail_proxy(&mut self, kind: ProxyFailureKind, code: &str, message: String) {
        self.proxy_ok = false;
        self.target_ok = false;
        self.failure_kind = Some(kind);
        self.error_code = Some(code.to_string());
        self.error_message = Some(message);
    }

    /// Mark a target-side failure. The proxy itself is FINE — this is
    /// exactly the case PART 3 must not treat as a dead proxy.
    fn fail_target(&mut self, kind: ProxyFailureKind, code: &str, message: String) {
        self.proxy_ok = true;
        self.target_ok = false;
        self.failure_kind = Some(kind);
        self.error_code = Some(format!("TARGET_{code}"));
        self.error_message = Some(message);
    }

    /// Attach informational error info to an otherwise-successful
    /// attempt (e.g. the proxy HTTP check worked but its HTTPS tunnel
    /// did not). Does NOT change the outcome.
    fn note_warning(&mut self, code: &str, message: String) {
        self.error_code = Some(code.to_string());
        self.error_message = Some(message);
    }
}

fn duration_ms(d: Duration) -> u32 {
    d.as_millis().try_into().unwrap_or(u32::MAX)
}

/// Fallback classification of a proxy-side failure message into a
/// structured `ProxyFailureKind`. Used by the HTTP proxy path, where
/// the underlying helpers only produce `String` errors.
fn classify_proxy_message(message: &str) -> ProxyFailureKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        ProxyFailureKind::Timeout
    } else if lower.contains("connection refused") {
        ProxyFailureKind::ConnectionRefused
    } else if lower.contains("tls") {
        ProxyFailureKind::TlsFailure
    } else if lower.contains("authentication") || lower.contains("auth") {
        ProxyFailureKind::AuthFailed
    } else if lower.contains("dns") || lower.contains("resolve") {
        ProxyFailureKind::DnsFailure
    } else {
        ProxyFailureKind::ProtocolFailure
    }
}

/// Stable string code for a proxy-side failure, matching the codes the
/// pre-PART-3 `error_code` function produced so existing consumers
/// (dashboard humanize logic, `attempted` billing predicate) keep
/// working unchanged.
fn proxy_failure_code(scheme: &ProxyScheme, kind: ProxyFailureKind) -> &'static str {
    match kind {
        ProxyFailureKind::AuthFailed => "AUTH_FAILED",
        ProxyFailureKind::ConnectionRefused => "CONNECTION_REFUSED",
        ProxyFailureKind::DnsFailure => "DNS_FAILED",
        ProxyFailureKind::TlsFailure => "TLS_FAILED",
        ProxyFailureKind::Timeout => "CONNECTION_TIMEOUT",
        ProxyFailureKind::ProtocolFailure => match scheme {
            ProxyScheme::Http => "HTTP_PROXY_FAILED",
            ProxyScheme::Https => "HTTPS_PROXY_FAILED",
            ProxyScheme::Socks4 => "SOCKS4_FAILED",
            ProxyScheme::Socks5 => "SOCKS5_FAILED",
        },
        _ => "HTTP_PROXY_FAILED",
    }
}

/// Classify a SOCKS handshake failure message into a structured kind.
fn classify_socks_error(message: &str) -> ProxyFailureKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        ProxyFailureKind::Timeout
    } else if lower.contains("authentication") {
        ProxyFailureKind::AuthFailed
    } else {
        ProxyFailureKind::ProtocolFailure
    }
}

/// Convert an `io::Error` from a TCP connect attempt into a
/// `(kind, code, message)` triple.
fn classify_connect_io_error(err: &std::io::Error) -> (ProxyFailureKind, &'static str, String) {
    if err.kind() == std::io::ErrorKind::ConnectionRefused {
        (
            ProxyFailureKind::ConnectionRefused,
            "CONNECTION_REFUSED",
            err.to_string(),
        )
    } else {
        (
            ProxyFailureKind::ProtocolFailure,
            "CONNECTION_FAILED",
            err.to_string(),
        )
    }
}

fn fail_connect_timeout(attempt: &mut CheckAttempt) {
    attempt.fail_proxy(
        ProxyFailureKind::Timeout,
        "CONNECTION_TIMEOUT",
        format!("connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"),
    );
}

fn build_proxy_result(
    entry: &ProxyEntry,
    started: Instant,
    attempt: CheckAttempt,
) -> ProxyResult {
    // Compute the outcome BEFORE moving any fields out of `attempt`.
    // `attempt.outcome()` borrows `self`, and several of the fields we
    // move into the result (`error_code`, `error_message`) are
    // non-Copy — reading `outcome` after moving them would be a partial
    // move error.
    let outcome = attempt.outcome();
    let reachable = outcome.fully_working();
    let total_time_ms = elapsed_ms(started);

    let mut safe_error = attempt.error_message;
    if let Some(message) = safe_error.as_mut() {
        message.truncate(512);
    }

    ProxyResult {
        host: entry.host().as_str().to_string(),
        port: entry.port(),
        scheme: proxy_scheme_str(entry.scheme()).to_string(),
        reachable,
        latency_ms: Some(total_time_ms),
        supports_https: attempt.supports_https,
        anonymity: None,
        country_code: None,
        error_code: attempt.error_code,
        error_message: safe_error,
        outcome: Some(outcome),
        failure_kind: attempt.failure_kind,
        connection_latency_ms: attempt.connection_latency_ms,
        tls_handshake_latency_ms: attempt.tls_handshake_latency_ms,
        ttfb_ms: attempt.ttfb_ms,
        total_time_ms: Some(total_time_ms),
        http_status_code: attempt.http_status_code,
    }
}

fn proxy_result(
    entry: &ProxyEntry,
    started: Instant,
    reachable: bool,
    supports_https: bool,
    error: Option<String>,
) -> ProxyResult {
    let mut safe_error = error;
    if let Some(message) = safe_error.as_mut() {
        message.truncate(512);
    }
    let error_code = safe_error
        .as_deref()
        .map(|e| legacy_error_code(entry.scheme(), e).to_string());
    ProxyResult {
        host: entry.host().as_str().to_string(),
        port: entry.port(),
        scheme: proxy_scheme_str(entry.scheme()).to_string(),
        reachable,
        latency_ms: Some(elapsed_ms(started)),
        supports_https,
        anonymity: None,
        country_code: None,
        error_code,
        error_message: safe_error,
        // PART 3 fields — placeholders for now. Block B will populate
        // these with actual measurements and structured classification.
        outcome: None,
        failure_kind: None,
        connection_latency_ms: None,
        tls_handshake_latency_ms: None,
        ttfb_ms: None,
        total_time_ms: None,
        http_status_code: None,
    }
}

async fn write_all_timeout<S>(
    stream: &mut S,
    data: &[u8],
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    timeout(timeout_duration, stream.write_all(data))
        .await
        .map_err(|_| "timed out writing to proxy".to_string())?
        .map_err(|e| format!("failed to write to proxy: {e}"))?;

    Ok(())
}

async fn read_exact_timeout<S>(
    stream: &mut S,
    buf: &mut [u8],
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + Unpin,
{
    timeout(timeout_duration, stream.read_exact(buf))
        .await
        .map_err(|_| "timed out reading from proxy".to_string())?
        .map_err(|e| format!("failed to read from proxy: {e}"))?;

    Ok(())
}

async fn read_http_header<S>(
    stream: &mut S,
    timeout_duration: Duration,
) -> Result<(String, Duration), String>
where
    S: AsyncRead + Unpin,
{
    let start = Instant::now();
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let mut first_byte_time: Option<Duration> = None;

    loop {
        let n = timeout(timeout_duration, stream.read(&mut chunk))
            .await
            .map_err(|_| "timed out waiting for proxy response".to_string())?
            .map_err(|e| format!("failed to read proxy response: {e}"))?;

        if n == 0 {
            return Err("proxy closed connection before completing response headers".to_string());
        }

        if first_byte_time.is_none() {
            first_byte_time = Some(start.elapsed());
        }

        if buf.len().saturating_add(n) > MAX_PROXY_RESPONSE_HEADER_BYTES {
            return Err("proxy response headers exceeded size limit".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);

        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let text =
        String::from_utf8(buf).map_err(|_| "proxy response was not valid UTF-8".to_string())?;
    Ok((text, first_byte_time.unwrap_or_default()))
}

fn parse_status_code(header: &str) -> Result<u16, String> {
    let first_line = header.lines().next().ok_or("empty proxy response")?;
    let mut parts = first_line.split_whitespace();

    let version = parts.next().ok_or("missing HTTP version")?;
    if !version.starts_with("HTTP/") {
        return Err(format!("invalid HTTP version '{version}'"));
    }

    let status = parts
        .next()
        .ok_or("missing HTTP status code")?
        .parse::<u16>()
        .map_err(|_| "invalid HTTP status code".to_string())?;

    Ok(status)
}

fn proxy_authorization_header(creds: Option<&ProxyCredentials>) -> String {
    match creds {
        Some(creds) => {
            let token = format!("{}:{}", creds.username(), creds.password());
            format!(
                "Proxy-Authorization: Basic {}\r\n",
                base64::engine::general_purpose::STANDARD.encode(token.as_bytes())
            )
        }
        None => String::new(),
    }
}

/// Outcome of a successful HTTP-level verification step.
#[derive(Debug)]
struct HttpVerifySuccess {
    status: u16,
    /// Time from when this verify step started to when the first
    /// response byte was read. Closest thing to a true TTFB in this
    /// codebase.
    ttfb: Duration,
}

/// Failure of an HTTP-level verification step, split by WHO failed:
/// the proxy's authentication, the proxy's protocol handling, or the
/// target through the proxy. The distinction is what PART 3 is about.
#[derive(Debug)]
enum VerifyHttpError {
    /// Proxy rejected credentials (HTTP 401/407).
    ///
    /// Carries the TTFB measured while reading the proxy's reply —
    /// even on failure, we DID get a well-formed HTTP response back,
    /// and the user needs to see how long it took.
    Auth { message: String, ttfb: Duration },
    /// Proxy spoke HTTP poorly (bad status line, oversized headers,
    /// connection closed early, malformed UTF-8, write failed).
    /// No TTFB is carried: the failure may have happened before any
    /// response byte arrived.
    Proxy(String),
    /// Proxy returned a non-success status for a well-formed request.
    /// The proxy is alive; the target through it did not work.
    /// Carries TTFB for the same reason as `Auth`.
    Target {
        status: u16,
        message: String,
        ttfb: Duration,
    },
}

async fn verify_http_proxy_absolute_form<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<HttpVerifySuccess, VerifyHttpError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "GET http://{HTTPS_VERIFY_TARGET_HOST}/ HTTP/1.1\r\nHost: {HTTPS_VERIFY_TARGET_HOST}\r\nUser-Agent: PingVertex/1.0\r\nConnection: close\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration)
        .await
        .map_err(VerifyHttpError::Proxy)?;
    let (header, ttfb) = read_http_header(stream, timeout_duration)
        .await
        .map_err(VerifyHttpError::Proxy)?;
    let status = parse_status_code(&header).map_err(VerifyHttpError::Proxy)?;

    if status == 401 || status == 407 {
        return Err(VerifyHttpError::Auth {
            message: format!("proxy authentication failed with HTTP status {status}"),
            ttfb,
        });
    }
    if !(200..400).contains(&status) {
        return Err(VerifyHttpError::Target {
            status,
            message: format!("proxy returned HTTP status {status}"),
            ttfb,
        });
    }
    Ok(HttpVerifySuccess { status, ttfb })
}

async fn verify_http_connect_tunnel<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<HttpVerifySuccess, VerifyHttpError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let target = format!("{HTTPS_VERIFY_TARGET_HOST}:{HTTPS_VERIFY_TARGET_PORT}");
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nUser-Agent: PingVertex/1.0\r\nProxy-Connection: keep-alive\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration)
        .await
        .map_err(VerifyHttpError::Proxy)?;
    let (header, ttfb) = read_http_header(stream, timeout_duration)
        .await
        .map_err(VerifyHttpError::Proxy)?;
    let status = parse_status_code(&header).map_err(VerifyHttpError::Proxy)?;

    if status == 401 || status == 407 {
        return Err(VerifyHttpError::Auth {
            message: format!("proxy authentication failed with CONNECT status {status}"),
            ttfb,
        });
    }
    if !(200..300).contains(&status) {
        return Err(VerifyHttpError::Target {
            status,
            message: format!("proxy returned CONNECT status {status}"),
            ttfb,
        });
    }
    Ok(HttpVerifySuccess { status, ttfb })
}

async fn tls_handshake<S>(
    stream: S,
    hostname: &str,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let connector = crate::proxy_tls::tls_connector().map_err(|e| e.to_string())?;
    let server_name = ServerName::try_from(hostname.to_string())
        .map_err(|_| "invalid TLS SNI hostname".to_string())?;

    timeout(timeout_duration, connector.connect(server_name, stream))
        .await
        .map_err(|_| "timed out during TLS handshake".to_string())?
        .map_err(|e| format!("TLS handshake failed: {e}"))?;

    Ok(())
}

/// Verifies a SOCKS4 (or, by extension, SOCKS4a) proxy.
///
/// SOCKS4 has no native hostname support in the original spec; the widely
/// deployed SOCKS4a extension (which almost every real-world SOCKS4
/// server implements) passes the destination hostname in place of the
/// DSTIP field, using the sentinel address `0.0.0.x` (with `x` nonzero)
/// to signal "this is a SOCKS4a request; the hostname follows". We emit a
/// SOCKS4a request because the verification target is a hostname
/// (`example.com`), which means the proxy — not this worker — does the
/// DNS resolution, exactly as a real user of the proxy would want.
///
/// Protocol reference (SOCKS4a):
///   Request:  VN=4, CD=1 (CONNECT), DSTPORT (2 bytes BE),
///             DSTIP (4 bytes = 0.0.0.x, x nonzero), USERID (NUL-terminated),
///             then DOMAIN (NUL-terminated).
///   Response: exactly 8 bytes: VN=0, CD (0x5A=granted, 0x5B=rejected,
///             0x5C=identd-unreachable, 0x5D=identd-mismatch),
///             DSTPORT (2 bytes), DSTIP (4 bytes).
///
/// AUTHENTICATION: SOCKS4 has no password field. The USERID field is the
/// only place a "username" can be transmitted. If credentials are present
/// we send `username` as USERID and silently drop `password` — the
/// protocol has nowhere to carry it. This matches every other SOCKS4
/// client in the wild.
///
/// The connection to the proxy itself is pinned to the already-validated
/// `SocketAddr` by the caller; this function does no DNS of its own.
async fn verify_socks4_proxy<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    target_host: &str,
    target_port: u16,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let host_bytes = target_host.as_bytes();
    if host_bytes.len() > 255 {
        // SOCKS4a hostnames are short in practice; a bound keeps the
        // Vec::with_capacity below honest and rejects anything absurd.
        return Err("target host too long for SOCKS4a".to_string());
    }

    let userid: &[u8] = match creds {
        Some(c) => c.username().as_bytes(),
        None => b"",
    };
    if userid.len() > 255 {
        return Err("SOCKS4 USERID too long".to_string());
    }

    // Build SOCKS4a CONNECT request. See doc comment above for the exact
    // byte layout and why DSTIP is 0.0.0.1 rather than a real address.
    let mut request =
        Vec::with_capacity(1 + 1 + 2 + 4 + userid.len() + 1 + host_bytes.len() + 1);
    request.push(0x04); // VN
    request.push(0x01); // CD = CONNECT
    request.extend_from_slice(&target_port.to_be_bytes());
    request.extend_from_slice(&[0, 0, 0, 1]); // DSTIP = 0.0.0.1 (SOCKS4a sentinel)
    request.extend_from_slice(userid);
    request.push(0x00); // USERID NUL terminator
    request.extend_from_slice(host_bytes);
    request.push(0x00); // DOMAIN NUL terminator

    write_all_timeout(stream, &request, timeout_duration).await?;

    let mut response = [0u8; 8];
    read_exact_timeout(stream, &mut response, timeout_duration).await?;

    let vn = response[0];
    let cd = response[1];

    if vn != 0x00 {
        return Err(format!(
            "SOCKS4 server returned invalid version byte 0x{vn:02x} (expected 0x00)"
        ));
    }

    match cd {
        0x5A => Ok(()), // 90 — request granted
        0x5B => Err("SOCKS4 request rejected or failed by the proxy".to_string()), // 91
        0x5C => Err(
            "SOCKS4 request rejected: proxy could not reach identd on the client".to_string(),
        ), // 92
        0x5D => Err(
            "SOCKS4 request rejected: identd on the client could not confirm the USERID"
                .to_string(),
        ), // 93
        other => Err(format!(
            "SOCKS4 server returned unrecognized reply code 0x{other:02x}"
        )),
    }
}

async fn verify_socks5_proxy<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    target_host: &str,
    target_port: u16,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let methods: &[u8] = if creds.is_some() {
        &[5, 1, 2]
    } else {
        &[5, 1, 0]
    };
    write_all_timeout(stream, methods, timeout_duration).await?;

    let mut method_resp = [0u8; 2];
    read_exact_timeout(stream, &mut method_resp, timeout_duration).await?;

    if method_resp[0] != 5 {
        return Err("SOCKS5 server returned invalid version".to_string());
    }
    if method_resp[1] == 0xFF {
        return Err("SOCKS5 server does not accept any authentication method".to_string());
    }

    match method_resp[1] {
        2 => {
            let creds = creds.ok_or_else(|| {
                "SOCKS5 server selected username/password auth but no credentials were provided"
                    .to_string()
            })?;
            let username = creds.username().as_bytes();
            let password = creds.password().as_bytes();

            if username.len() > 255 || password.len() > 255 {
                return Err("SOCKS5 credentials too long".to_string());
            }

            let mut auth_req = Vec::with_capacity(3 + username.len() + password.len());
            auth_req.push(1);
            auth_req.push(username.len() as u8);
            auth_req.extend_from_slice(username);
            auth_req.push(password.len() as u8);
            auth_req.extend_from_slice(password);

            write_all_timeout(stream, &auth_req, timeout_duration).await?;

            let mut auth_resp = [0u8; 2];
            read_exact_timeout(stream, &mut auth_resp, timeout_duration).await?;

            if auth_resp[0] != 1 || auth_resp[1] != 0 {
                return Err("SOCKS5 username/password authentication failed".to_string());
            }
        }
        0 => {}
        other => {
            return Err(format!(
                "SOCKS5 server selected unsupported authentication method {other}"
            ));
        }
    }

    let host_bytes = target_host.as_bytes();
    if host_bytes.len() > 255 {
        return Err("target host too long for SOCKS5".to_string());
    }

    let mut connect_req = Vec::with_capacity(7 + host_bytes.len());
    connect_req.push(5);
    connect_req.push(1);
    connect_req.push(0);
    connect_req.push(3);
    connect_req.push(host_bytes.len() as u8);
    connect_req.extend_from_slice(host_bytes);
    connect_req.extend_from_slice(&target_port.to_be_bytes());

    write_all_timeout(stream, &connect_req, timeout_duration).await?;

    let mut connect_head = [0u8; 4];
    read_exact_timeout(stream, &mut connect_head, timeout_duration).await?;

    if connect_head[0] != 5 {
        return Err("SOCKS5 connect reply had invalid version".to_string());
    }
    if connect_head[1] != 0 {
        return Err(format!(
            "SOCKS5 connect failed with reply code {}",
            connect_head[1]
        ));
    }

    match connect_head[3] {
        1 => {
            let mut rest = [0u8; 6];
            read_exact_timeout(stream, &mut rest, timeout_duration).await?;
        }
        4 => {
            let mut rest = [0u8; 18];
            read_exact_timeout(stream, &mut rest, timeout_duration).await?;
        }
        3 => {
            let mut len_buf = [0u8; 1];
            read_exact_timeout(stream, &mut len_buf, timeout_duration).await?;
            let len = len_buf[0] as usize;
            let mut addr = vec![0u8; len];
            read_exact_timeout(stream, &mut addr, timeout_duration).await?;
            let mut port_buf = [0u8; 2];
            read_exact_timeout(stream, &mut port_buf, timeout_duration).await?;
        }
        _ => return Err("SOCKS5 connect reply had invalid address type".to_string()),
    }

    Ok(())
}

/// Checks a single proxy entry and returns its `ProxyResult`.
///
/// Steps:
/// 1. Build a `scheme://host:port` URL from the entry's own fields and
///    run it through `resolve_and_validate_destination` — this performs
///    the DNS lookup exactly once, rejects any private/reserved resolved
///    address (Layer 2 SSRF defense, on top of `ProxyHost::new`'s
///    literal-IP check already performed at submission time), and
///    returns the pinned `SocketAddr` to connect to.
/// 2. Connect to that pinned address and perform the protocol-specific
///    verification described in the module docs.
///
/// A resolution/validation failure and a connection failure are both
/// reported as `reachable: false` with a descriptive `error`, never as a
/// panic or a propagated `Result` — one bad proxy entry must never stop
/// the rest of the job from being checked.
async fn check_single_entry(entry: &ProxyEntry) -> (ProxyResult, bool) {
    let target = format!(
        "{}://{}:{}",
        proxy_scheme_str(entry.scheme()),
        entry.host().as_str(),
        entry.port()
    );

    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_public_destination(&target).await {
        Ok(destination) => destination,
        Err(err) => {
            let mut attempt = CheckAttempt::new();
            let (kind, code) = if err.to_ascii_lowercase().contains("ssrf blocked") {
                (ProxyFailureKind::SsrfBlocked, "PRIVATE_IP_BLOCKED")
            } else {
                (ProxyFailureKind::InvalidInput, "INVALID_PROXY")
            };
            attempt.fail_proxy(kind, code, err);
            let result = build_proxy_result(entry, started, attempt);
            // Input was rejected before any network attempt — do NOT
            // charge PAYG for it (this is the existing `attempted` bit).
            return (result, false);
        }
    };

    let connect_timeout = Duration::from_secs(PER_ENTRY_TIMEOUT_SECS);
    let protocol_timeout = Duration::from_secs(PROTOCOL_TIMEOUT_SECS);

    let attempt = match entry.scheme() {
        ProxyScheme::Http => {
            check_http_proxy(entry, &destination, connect_timeout, protocol_timeout).await
        }
        ProxyScheme::Https => {
            check_https_proxy(entry, &destination, connect_timeout, protocol_timeout).await
        }
        ProxyScheme::Socks4 => {
            check_socks4_proxy(entry, &destination, connect_timeout, protocol_timeout).await
        }
        ProxyScheme::Socks5 => {
            check_socks5_proxy(entry, &destination, connect_timeout, protocol_timeout).await
        }
    };

    let result = build_proxy_result(entry, started, attempt);
    (result, true)
}

async fn check_http_proxy(
    entry: &ProxyEntry,
    destination: &crate::ssrf::ResolvedDestination,
    connect_timeout: Duration,
    protocol_timeout: Duration,
) -> CheckAttempt {
    let mut attempt = CheckAttempt::new();

    // --- TCP connect to proxy ---
    let t0 = Instant::now();
    let mut stream =
        match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
            Ok(Ok(s)) => s,
            Ok(Err(err)) => {
                attempt.connection_latency_ms = Some(elapsed_ms(t0));
                let (kind, code, msg) = classify_connect_io_error(&err);
                attempt.fail_proxy(kind, code, msg);
                return attempt;
            }
            Err(_) => {
                attempt.connection_latency_ms = Some(elapsed_ms(t0));
                fail_connect_timeout(&mut attempt);
                return attempt;
            }
        };
    attempt.connection_latency_ms = Some(elapsed_ms(t0));

    // --- HTTP absolute-form GET ---
    match verify_http_proxy_absolute_form(&mut stream, entry.credentials(), protocol_timeout)
        .await
    {
        Ok(success) => {
            attempt.ttfb_ms = Some(duration_ms(success.ttfb));
            attempt.http_status_code = Some(success.status);
            attempt.succeed();
        }
        Err(VerifyHttpError::Auth { message, ttfb }) => {
            attempt.ttfb_ms = Some(duration_ms(ttfb));
            attempt.fail_proxy(ProxyFailureKind::AuthFailed, "AUTH_FAILED", message);
            return attempt;
        }
        Err(VerifyHttpError::Proxy(message)) => {
            let kind = classify_proxy_message(&message);
            let code = proxy_failure_code(entry.scheme(), kind);
            attempt.fail_proxy(kind, code, message);
            return attempt;
        }
        Err(VerifyHttpError::Target {
            status,
            message,
            ttfb,
        }) => {
            attempt.http_status_code = Some(status);
            attempt.ttfb_ms = Some(duration_ms(ttfb));
            attempt.fail_target(ProxyFailureKind::TargetHttpError, "HTTP_ERROR", message);
            return attempt;
        }
    }

    // --- HTTP worked. Now test HTTPS capability via a CONNECT tunnel. ---
    // A failure in this section is a TARGET-side warning (the proxy is
    // still Working); it only affects `supports_https`.
    let mut tunnel_stream =
        match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
            Ok(Ok(s)) => s,
            Ok(Err(err)) => {
                attempt.note_warning(
                    "TARGET_CONNECT_FAILED",
                    format!("HTTPS tunnel connection failed: {err}"),
                );
                return attempt;
            }
            Err(_) => {
                attempt.note_warning(
                    "TARGET_CONNECT_TIMEOUT",
                    "HTTPS tunnel connection timed out".to_string(),
                );
                return attempt;
            }
        };

    match verify_http_connect_tunnel(&mut tunnel_stream, entry.credentials(), protocol_timeout)
        .await
    {
        Ok(_success) => {}
        Err(VerifyHttpError::Auth { message, .. }) => {
            attempt.note_warning(
                "TARGET_CONNECT_AUTH_FAILED",
                format!("HTTPS tunnel authentication failed: {message}"),
            );
            return attempt;
        }
        Err(VerifyHttpError::Proxy(message)) => {
            attempt.note_warning(
                "TARGET_CONNECT_FAILED",
                format!("HTTPS tunnel failed: {message}"),
            );
            return attempt;
        }
        Err(VerifyHttpError::Target {
            status, message, ..
        }) => {
            attempt.note_warning(
                "TARGET_CONNECT_HTTP_ERROR",
                format!("HTTPS tunnel returned HTTP {status}: {message}"),
            );
            return attempt;
        }
    }

    // --- TLS handshake through tunnel to target ---
    let t0 = Instant::now();
    match tls_handshake(tunnel_stream, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await {
        Ok(()) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.supports_https = true;
        }
        Err(err) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.note_warning(
                "TARGET_TLS_FAILED",
                format!("HTTPS target TLS failed: {err}"),
            );
        }
    }

    attempt
}

async fn check_https_proxy(
    entry: &ProxyEntry,
    destination: &crate::ssrf::ResolvedDestination,
    connect_timeout: Duration,
    protocol_timeout: Duration,
) -> CheckAttempt {
    let mut attempt = CheckAttempt::new();

    // --- TCP connect ---
    let t0 = Instant::now();
    let tcp = match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => {
            attempt.connection_latency_ms = Some(elapsed_ms(t0));
            let (kind, code, msg) = classify_connect_io_error(&err);
            attempt.fail_proxy(kind, code, msg);
            return attempt;
        }
        Err(_) => {
            attempt.connection_latency_ms = Some(elapsed_ms(t0));
            fail_connect_timeout(&mut attempt);
            return attempt;
        }
    };
    attempt.connection_latency_ms = Some(elapsed_ms(t0));

    // --- TLS handshake to proxy ---
    let connector = match crate::proxy_tls::tls_connector() {
        Ok(c) => c,
        Err(err) => {
            attempt.fail_proxy(ProxyFailureKind::Other, "TLS_FAILED", err.to_string());
            return attempt;
        }
    };
    let server_name = match ServerName::try_from(entry.host().as_str().to_string()) {
        Ok(n) => n,
        Err(err) => {
            attempt.fail_proxy(ProxyFailureKind::InvalidInput, "INVALID_PROXY", err.to_string());
            return attempt;
        }
    };
    let t0 = Instant::now();
    let mut proxy_tls = match timeout(protocol_timeout, connector.connect(server_name, tcp)).await {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.fail_proxy(
                ProxyFailureKind::TlsFailure,
                "TLS_FAILED",
                format!("TLS to proxy failed: {err}"),
            );
            return attempt;
        }
        Err(_) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.fail_proxy(
                ProxyFailureKind::Timeout,
                "CONNECTION_TIMEOUT",
                "TLS handshake with proxy timed out".to_string(),
            );
            return attempt;
        }
    };
    attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
    // TCP + TLS to proxy both worked — the proxy is alive up to here.
    attempt.proxy_ok = true;

    // --- CONNECT tunnel to target ---
    match verify_http_connect_tunnel(&mut proxy_tls, entry.credentials(), protocol_timeout).await {
        Ok(success) => {
            attempt.ttfb_ms = Some(duration_ms(success.ttfb));
            attempt.http_status_code = Some(success.status);
        }
        Err(VerifyHttpError::Auth { message, ttfb }) => {
            attempt.ttfb_ms = Some(duration_ms(ttfb));
            attempt.fail_proxy(ProxyFailureKind::AuthFailed, "AUTH_FAILED", message);
            return attempt;
        }
        Err(VerifyHttpError::Proxy(message)) => {
            attempt.fail_proxy(
                ProxyFailureKind::ProtocolFailure,
                "HTTPS_PROXY_FAILED",
                format!("CONNECT through proxy failed: {message}"),
            );
            return attempt;
        }
        Err(VerifyHttpError::Target {
            status,
            message,
            ttfb,
        }) => {
            attempt.http_status_code = Some(status);
            attempt.ttfb_ms = Some(duration_ms(ttfb));
            attempt.fail_target(ProxyFailureKind::TargetHttpError, "CONNECT_HTTP_ERROR", message);
            return attempt;
        }
    }

    // --- TLS handshake through tunnel to target ---
    let t0 = Instant::now();
    match tls_handshake(proxy_tls, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await {
        Ok(()) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.supports_https = true;
            attempt.succeed();
        }
        Err(err) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.fail_target(
                ProxyFailureKind::TlsFailure,
                "TLS_FAILED",
                format!("TLS to target through proxy failed: {err}"),
            );
        }
    }

    attempt
}

async fn check_socks4_proxy(
    entry: &ProxyEntry,
    destination: &crate::ssrf::ResolvedDestination,
    connect_timeout: Duration,
    protocol_timeout: Duration,
) -> CheckAttempt {
    let mut attempt = CheckAttempt::new();

    let t0 = Instant::now();
    let mut stream =
        match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
            Ok(Ok(s)) => s,
            Ok(Err(err)) => {
                attempt.connection_latency_ms = Some(elapsed_ms(t0));
                let (kind, code, msg) = classify_connect_io_error(&err);
                attempt.fail_proxy(kind, code, msg);
                return attempt;
            }
            Err(_) => {
                attempt.connection_latency_ms = Some(elapsed_ms(t0));
                fail_connect_timeout(&mut attempt);
                return attempt;
            }
        };
    attempt.connection_latency_ms = Some(elapsed_ms(t0));

    match verify_socks4_proxy(
        &mut stream,
        entry.credentials(),
        HTTPS_VERIFY_TARGET_HOST,
        HTTPS_VERIFY_TARGET_PORT,
        protocol_timeout,
    )
    .await
    {
        Ok(()) => {
            attempt.proxy_ok = true;
        }
        Err(err) => {
            let kind = classify_socks_error(&err);
            let code = proxy_failure_code(entry.scheme(), kind);
            attempt.fail_proxy(kind, code, err);
            return attempt;
        }
    }

    let t0 = Instant::now();
    match tls_handshake(stream, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await {
        Ok(()) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.supports_https = true;
            attempt.succeed();
        }
        Err(err) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.fail_target(
                ProxyFailureKind::TlsFailure,
                "TLS_FAILED",
                format!("SOCKS4 tunnel connected but TLS to target failed: {err}"),
            );
        }
    }

    attempt
}

async fn check_socks5_proxy(
    entry: &ProxyEntry,
    destination: &crate::ssrf::ResolvedDestination,
    connect_timeout: Duration,
    protocol_timeout: Duration,
) -> CheckAttempt {
    let mut attempt = CheckAttempt::new();

    let t0 = Instant::now();
    let mut stream =
        match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
            Ok(Ok(s)) => s,
            Ok(Err(err)) => {
                attempt.connection_latency_ms = Some(elapsed_ms(t0));
                let (kind, code, msg) = classify_connect_io_error(&err);
                attempt.fail_proxy(kind, code, msg);
                return attempt;
            }
            Err(_) => {
                attempt.connection_latency_ms = Some(elapsed_ms(t0));
                fail_connect_timeout(&mut attempt);
                return attempt;
            }
        };
    attempt.connection_latency_ms = Some(elapsed_ms(t0));

    match verify_socks5_proxy(
        &mut stream,
        entry.credentials(),
        HTTPS_VERIFY_TARGET_HOST,
        HTTPS_VERIFY_TARGET_PORT,
        protocol_timeout,
    )
    .await
    {
        Ok(()) => {
            attempt.proxy_ok = true;
        }
        Err(err) => {
            let kind = classify_socks_error(&err);
            let code = proxy_failure_code(entry.scheme(), kind);
            attempt.fail_proxy(kind, code, err);
            return attempt;
        }
    }

    let t0 = Instant::now();
    match tls_handshake(stream, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await {
        Ok(()) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.supports_https = true;
            attempt.succeed();
        }
        Err(err) => {
            attempt.tls_handshake_latency_ms = Some(elapsed_ms(t0));
            attempt.fail_target(
                ProxyFailureKind::TlsFailure,
                "TLS_FAILED",
                format!("SOCKS5 tunnel connected but TLS to target failed: {err}"),
            );
        }
    }

    attempt
}

/// Runs every entry of `job` through `check_single_entry`, persisting
/// each result and the job's overall progress/status as it goes.
///
/// `job` must already have been persisted (via
/// `ProxyCheckJobRepository::save_job`, called by the application layer's
/// submit use case) and `entries` must be exactly the entries belonging
/// to it. This function's only responsibilities are: transition the job
/// to `Running` (an optimistic-lock no-op if some other worker already
/// picked it up — see `ProxyCheckJob::mark_running`'s doc comment), check
/// every entry concurrently (bounded by `MAX_CONCURRENT_CHECKS_PER_JOB`),
/// persist each entry's result via `update_entry`, and finally mark the
/// job `Completed` and persist that.
///
/// Dispatch (how a submitted job reaches this function in the first
/// place — polling, LISTEN/NOTIFY, a channel, etc.) is out of scope here
/// and left to a separate prompt, per the task boundary for this module.
pub async fn run_proxy_check_job<R>(
    repository: &R,
    mut job: ProxyCheckJob,
    entries: Vec<ProxyEntry>,
    worker_id: &str,
    metering: Option<Arc<MeterUsageUseCase>>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: ProxyCheckJobRepository + ?Sized,
{
    // The worker already performed the atomic pending -> running claim.
    // The executor MUST NOT own or repeat that state transition.
    if job.status() != domain::ProxyCheckJobStatus::Running {
        info!(job_id = %job.id(), "proxy check job is not running; skipping");
        return Ok(());
    }

    if entries.len() != job.total_count() as usize {
        let mut failed = job.clone();
        if failed.mark_failed(format!(
            "proxy entry count mismatch: expected {}, loaded {}",
            job.total_count(),
            entries.len()
        )) {
            repository.update_job(&failed).await?;
        }
        return Ok(());
    }

    let persisted_successes = entries
        .iter()
        .filter(|e| e.result().is_some_and(|r| r.reachable))
        .count() as u32;
    let persisted_failures = entries
        .iter()
        .filter(|e| e.result().is_some_and(|r| !r.reachable))
        .count() as u32;
    job.rebuild_progress(persisted_successes, persisted_failures);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS_PER_JOB));
    let mut tasks = JoinSet::new();
    for entry in entries {
        let had_existing_result = entry.result().is_some();
        let semaphore = Arc::clone(&semaphore);
        let metering = metering.clone();
        let job_for_task = job.clone();

        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore remains open");
            let mut entry = entry;
            let entry_id = entry.id();
            let job_id = job_for_task.id();
            let (result, billing_blocked) = if let Some(metering) = metering.clone() {
                let request_id = format!("proxy-check:{}:{}", job_id, entry_id);
                if had_existing_result && metering.find_by_request(job_for_task.tenant_id().0, &request_id).await?.is_some() {
                    (entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("PAYG execution was already finalized".to_owned()))), false)
                } else {
                match metering.execute_classified_attempt_with_output(
                    MeterUsageInput {
                        tenant_id: job_for_task.tenant_id().0,
                        operation: domain::PaygOperation::ProxyMonitoring.as_str().to_owned(),
                        units: 1,
                        request_id,

            monitor_id: None,
            execution_id: None,
            pricing_context: None,
        },
                    || async {
                        if had_existing_result {
                            let result = entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("persisted proxy result missing".to_owned())));
                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted: true, error: None, egress_bytes: 0, actual_units: None }, result))
                        } else {
                            let (result, attempted) = check_single_entry(&entry).await;
                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted, error: None, egress_bytes: 0, actual_units: None }, result))
                        }
                    },
                ).await {
                    Ok((_, Some(result))) => (result, false),
                    Ok((_, None)) => {
                        (entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("PAYG execution was already finalized".to_owned()))), false)
                    }
                    Err(application::ChargeForUsageError::InsufficientBalance) => {
                        let result = entry.result().cloned().unwrap_or_else(|| proxy_result(
                            &entry, Instant::now(), false, false,
                            Some("PAYG balance exhausted; top up the wallet to resume proxy checks".to_owned()),
                        ));
                        (result, true)
                    }
                    Err(application::ChargeForUsageError::OperationNotAttempted(reason)) => {
                        let result = proxy_result(&entry, Instant::now(), false, false, Some(reason));
                        (result, false)
                    }
                    Err(err) => return Err(Box::new(err) as Box<dyn Error + Send + Sync>),
                }
                }
            } else {
                let (result, _) = check_single_entry(&entry).await;
                (result, false)
            };
            entry.set_result(result);
            Ok((entry, !had_existing_result, billing_blocked))
        });
    }

    let mut fatal_reason: Option<String> = None;
    let mut billing_blocked = false;
    let heartbeat_every = Duration::from_secs(15);

    loop {
        match timeout(heartbeat_every, tasks.join_next()).await {
            Ok(Some(Ok(Ok((entry, should_count, entry_billing_blocked))))) => {
                billing_blocked |= entry_billing_blocked;
                if let Err(err) = repository.update_entry(&entry, job.tenant_id()).await {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to persist proxy entry result: {err}")
                    });
                    error!(job_id=%job.id(), entry_id=%entry.id(), error=%err, "proxy result persistence failed");
                    continue;
                }

                if should_count {
                    job.record_result(entry.result().is_some_and(|r| r.reachable));
                }

                // Persist progress periodically rather than once per proxy;
                // the entry result itself is already durably stored. This keeps
                // bulk jobs cheap while the separate heartbeat preserves the lease.
                if (job.checked_count() == job.total_count()
                    || job.checked_count().is_multiple_of(10))
                    && let Err(err) = repository.update_job(&job).await
                {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to persist proxy check job progress: {err}")
                    });
                    error!(
                        job_id=%job.id(),
                        error=%err,
                        "proxy job progress persistence failed"
                    );
                }

                // Lease ownership is separate from job progress so a failed
                // progress write cannot accidentally hide the heartbeat.
                match repository.heartbeat(job.id(), worker_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        fatal_reason.get_or_insert_with(|| "proxy job lease was lost".to_string());
                        break;
                    }
                    Err(err) => {
                        fatal_reason.get_or_insert_with(|| {
                            format!("failed to heartbeat proxy check job: {err}")
                        });
                        error!(job_id=%job.id(), error=%err, "proxy job heartbeat failed");
                        break;
                    }
                }
            }
            Ok(Some(Ok(Err(err)))) => {
                fatal_reason.get_or_insert_with(|| format!("proxy entry task failed: {err}"));
                warn!(job_id=%job.id(), error=%err, "proxy entry task failed");
            }
            Ok(Some(Err(join_err))) => {
                fatal_reason
                    .get_or_insert_with(|| format!("proxy entry task join failed: {join_err}"));
                warn!(job_id=%job.id(), error=%join_err, "proxy entry task join failed");
            }
            Ok(None) => break,
            Err(_) => match repository.heartbeat(job.id(), worker_id).await {
                Ok(true) => {}
                Ok(false) => {
                    fatal_reason.get_or_insert_with(|| "proxy job lease was lost".to_string());
                    break;
                }
                Err(err) => {
                    fatal_reason.get_or_insert_with(|| {
                        format!("failed to heartbeat proxy check job: {err}")
                    });
                    break;
                }
            },
        }
    }

    if billing_blocked {
        // Keep the durable job Running when one or more entries were blocked by
        // insufficient PAYG balance. The lease will expire, stale-job recovery
        // will return it to Pending, and a later sweep can retry after top-up.
        repository.update_job(&job).await?;
        return Ok(());
    }

    if let Some(reason) = fatal_reason {
        if job.status() == domain::ProxyCheckJobStatus::Running && job.mark_failed(reason) {
            repository.update_job(&job).await?;
        }
        return Ok(());
    }

    if job.checked_count() != job.total_count()
        || job.checked_count() != job.successful_count().saturating_add(job.failed_count())
    {
        if job.mark_failed("proxy result persistence incomplete") {
            repository.update_job(&job).await?;
        }
        return Ok(());
    }

    if job.mark_completed() {
        repository.update_job(&job).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::IdGenerator;
    use domain::shared::ids::{TenantId, UserId};
    use domain::value_objects::ProxyHost;
    use infra::{InMemoryProxyCheckJobRepository, UuidIdGenerator};
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn run_proxy_check_job_marks_job_completed_and_records_entry_result() {
        let repo = InMemoryProxyCheckJobRepository::new();
        let id_generator = UuidIdGenerator::new();

        let user_id = UserId::new();
        let job_id = id_generator.next_proxy_check_job_id().await;
        let job = ProxyCheckJob::new(job_id, user_id, TenantId::new(), 1);

        let entry_id = id_generator.next_proxy_entry_id().await;
        let host = ProxyHost::new("localhost").unwrap();
        let entry = ProxyEntry::new(entry_id, job_id, host, 8080, ProxyScheme::Http, None).unwrap();

        repo.save_job(&job, &[entry]).await.unwrap();

        // The worker owns the atomic Pending -> Running claim.
        assert!(repo.try_mark_running(job_id, "test-worker").await.unwrap());

        let running_job = repo
            .find_job_by_id(job_id, job.tenant_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(running_job.status(), domain::ProxyCheckJobStatus::Running);

        let entries = repo
            .find_entries_by_job_id(job_id, job.tenant_id())
            .await
            .unwrap();

        run_proxy_check_job(&repo, running_job, entries, "test-worker", None)
            .await
            .unwrap();

        let job_after = repo
            .find_job_by_id(job_id, job.tenant_id())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job_after.status(), domain::ProxyCheckJobStatus::Completed);
        assert_eq!(job_after.checked_count(), 1);

        let entries_after = repo
            .find_entries_by_job_id(job_id, job.tenant_id())
            .await
            .unwrap();
        assert_eq!(entries_after.len(), 1);
        assert!(entries_after[0].result().is_some());
    }

    #[test]
    fn parse_status_code_accepts_200() {
        assert_eq!(parse_status_code("HTTP/1.1 200 OK\r\n\r\n").unwrap(), 200);
    }

    #[test]
    fn parse_status_code_rejects_non_http() {
        assert!(parse_status_code("SOCKS5\r\n").is_err());
    }

    #[tokio::test]
    async fn http_proxy_verification_accepts_2xx() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await;
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let creds = ProxyCredentials::new("user", "pass").ok();

        assert!(
            verify_http_proxy_absolute_form(&mut stream, creds.as_ref(), Duration::from_secs(2))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn http_proxy_verification_rejects_oversized_headers() {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream.read(&mut [0u8; 4096]).await;
                let payload = vec![b'a'; MAX_PROXY_RESPONSE_HEADER_BYTES + 1024];
                let _ = stream.write_all(&payload).await;
                let _ = stream.flush().await;
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_proxy_absolute_form(&mut stream, None, Duration::from_secs(2)).await;

        let err = result.expect_err("oversized headers must fail");
        match err {
            VerifyHttpError::Proxy(msg) => {
                assert!(msg.contains("size limit"), "unexpected message: {msg}");
            }
            other => panic!("expected Proxy error, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------
    // SOCKS4 / SOCKS4a protocol tests.
    //
    // These verify the EXACT wire bytes verify_socks4_proxy sends and
    // how it interprets each reply code. This matters because SOCKS4 is
    // a hand-rolled protocol here — a wrong byte offset would silently
    // break every real SOCKS4 proxy in production. The fake server reads
    // a length the test computes up front (fixed header + USERID + NUL +
    // DOMAIN + NUL), replies with 8 bytes, and hands the captured
    // request back over a oneshot channel so assertions run in the test
    // task, not the spawned task (avoids the classic "assertion in
    // spawned task silently swallowed" test bug).
    // ---------------------------------------------------------------

    /// Drives `verify_socks4_proxy` against a fake server that replies
    /// with the supplied 8-byte response, and returns the request bytes
    /// the function actually wrote.
    ///
    /// `request_len` is computed by the caller from the exact `userid`
    /// and `target_host` it plans to pass in, since SOCKS4a is not
    /// length-prefixed.
    async fn run_socks4_case(
        reply: [u8; 8],
        creds: Option<ProxyCredentials>,
        target_host: &str,
        request_len: usize,
    ) -> (Result<(), String>, Vec<u8>) {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<u8>>();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut request = vec![0u8; request_len];
                if stream.read_exact(&mut request).await.is_ok() {
                    let _ = stream.write_all(&reply).await;
                    let _ = stream.flush().await;
                }
                let _ = tx.send(request);
            }
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result = verify_socks4_proxy(
            &mut stream,
            creds.as_ref(),
            target_host,
            443,
            Duration::from_secs(2),
        )
        .await;

        let request = rx.await.expect("fake server must capture the request");
        (result, request)
    }

    #[tokio::test]
    async fn socks4_proxy_verification_accepts_granted_reply_and_sends_socks4a_request() {
        // Layout for no credentials + "example.com":
        //   VN(1) + CD(1) + DSTPORT(2) + DSTIP(4)         = 8
        //   USERID("") + NUL                              = 1
        //   DOMAIN("example.com" = 11 bytes) + NUL        = 12
        //   total                                         = 21
        const REQUEST_LEN: usize = 21;

        let (result, request) = run_socks4_case(
            [0x00, 0x5A, 0x00, 0x50, 0, 0, 0, 0],
            None,
            "example.com",
            REQUEST_LEN,
        )
        .await;

        assert!(result.is_ok(), "expected Ok, got {result:?}");

        assert_eq!(request[0], 0x04, "VN must be 0x04");
        assert_eq!(request[1], 0x01, "CD must be 0x01 (CONNECT)");
        assert_eq!(
            &request[2..4],
            &443u16.to_be_bytes(),
            "DSTPORT must be 443, big-endian"
        );
        assert_eq!(
            &request[4..8],
            &[0, 0, 0, 1],
            "DSTIP must be the SOCKS4a sentinel 0.0.0.1"
        );
        assert_eq!(request[8], 0x00, "USERID must be empty + NUL terminator");
        assert_eq!(
            &request[9..20],
            b"example.com",
            "DOMAIN must carry the target hostname"
        );
        assert_eq!(request[20], 0x00, "DOMAIN must be NUL-terminated");
    }

    #[tokio::test]
    async fn socks4_proxy_verification_sends_username_as_userid_and_ignores_password() {
        // Layout with USERID = "user" (4 bytes):
        //   8 (fixed) + 4 + 1 (USERID NUL) + 11 + 1 (DOMAIN NUL) = 25
        const REQUEST_LEN: usize = 25;
        const PASSWORD_SENTINEL: &str = "secret-should-never-appear-on-the-wire";

        let creds =
            ProxyCredentials::new("user", PASSWORD_SENTINEL).expect("valid credentials");

        let (result, request) = run_socks4_case(
            [0x00, 0x5A, 0x00, 0x50, 0, 0, 0, 0],
            Some(creds),
            "example.com",
            REQUEST_LEN,
        )
        .await;

        assert!(result.is_ok(), "expected Ok, got {result:?}");

        assert_eq!(
            &request[8..13],
            b"user\0",
            "USERID must be the username followed by a NUL"
        );

        // The SOCKS4 protocol has no password field. If this assertion
        // ever fires, verify_socks4_proxy has started transmitting the
        // password somewhere it does not belong — a silent credential
        // leak to any third-party SOCKS4 proxy operator.
        let needle = PASSWORD_SENTINEL.as_bytes();
        assert!(
            !request.windows(needle.len()).any(|w| w == needle),
            "password must NEVER appear anywhere in a SOCKS4 request"
        );
    }

    #[tokio::test]
    async fn socks4_proxy_verification_rejects_all_failure_reply_codes() {
        const REQUEST_LEN: usize = 21;

        // 0x5B — request rejected or failed.
        let (result, _) = run_socks4_case(
            [0x00, 0x5B, 0, 0, 0, 0, 0, 0],
            None,
            "example.com",
            REQUEST_LEN,
        )
        .await;
        let err = result.expect_err("0x5B must be rejected");
        assert!(
            err.contains("rejected or failed"),
            "unexpected 0x5B error: {err}"
        );

        // 0x5C — proxy could not reach identd on the client.
        let (result, _) = run_socks4_case(
            [0x00, 0x5C, 0, 0, 0, 0, 0, 0],
            None,
            "example.com",
            REQUEST_LEN,
        )
        .await;
        let err = result.expect_err("0x5C must be rejected");
        assert!(
            err.contains("identd on the client"),
            "unexpected 0x5C error: {err}"
        );

        // 0x5D — identd could not confirm the USERID.
        let (result, _) = run_socks4_case(
            [0x00, 0x5D, 0, 0, 0, 0, 0, 0],
            None,
            "example.com",
            REQUEST_LEN,
        )
        .await;
        let err = result.expect_err("0x5D must be rejected");
        assert!(
            err.contains("could not confirm the USERID"),
            "unexpected 0x5D error: {err}"
        );
    }

    #[tokio::test]
    async fn socks4_proxy_verification_rejects_invalid_version_byte() {
        // A well-behaved SOCKS4 server always returns VN=0x00 in the
        // reply. Anything else means this is not really a SOCKS4 server
        // (or the reply is misaligned), and we must not accept it.
        let (result, _) = run_socks4_case(
            [0x05, 0x5A, 0, 0, 0, 0, 0, 0],
            None,
            "example.com",
            21,
        )
        .await;

        let err = result.expect_err("reply VN=0x05 must be rejected");
        assert!(
            err.contains("invalid version byte"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn socks4_proxy_verification_rejects_unrecognized_reply_code() {
        // 0x99 is not one of 0x5A/0x5B/0x5C/0x5D. We must fail closed
        // rather than treating an unknown byte as success.
        let (result, _) = run_socks4_case(
            [0x00, 0x99, 0, 0, 0, 0, 0, 0],
            None,
            "example.com",
            21,
        )
        .await;

        let err = result.expect_err("reply CD=0x99 must be rejected");
        assert!(
            err.contains("unrecognized reply code"),
            "unexpected error: {err}"
        );
    }

    // ---------------------------------------------------------------
    // PART 3 — CheckAttempt, classification helpers, VerifyHttpError
    // ---------------------------------------------------------------

    // -- CheckAttempt: outcome derivation --

    #[test]
    fn check_attempt_working_when_proxy_and_target_ok() {
        let mut a = CheckAttempt::new();
        a.succeed();
        assert_eq!(a.outcome(), ProxyCheckOutcome::Working);
        assert!(a.outcome().proxy_is_alive());
        assert!(a.outcome().fully_working());
        assert!(a.failure_kind.is_none());
        assert!(a.error_code.is_none());
    }

    #[test]
    fn check_attempt_target_failure_keeps_proxy_alive() {
        // THE central PART 3 invariant: when the proxy succeeded but the
        // target through it did not, the outcome must be
        // WorkingTargetUnreachable — the proxy is NOT dead.
        let mut a = CheckAttempt::new();
        a.proxy_ok = true;
        a.fail_target(
            ProxyFailureKind::Timeout,
            "TIMEOUT",
            "target timed out".to_string(),
        );

        assert_eq!(a.outcome(), ProxyCheckOutcome::WorkingTargetUnreachable);
        assert!(a.outcome().proxy_is_alive(), "target failure must not kill the proxy");
        assert!(!a.outcome().fully_working());
        assert!(a.outcome().is_target_failure());
        assert_eq!(a.failure_kind, Some(ProxyFailureKind::Timeout));
        // Target failures get prefixed so pre-PART-3 rows can be
        // retroactively classified by the domain helper.
        assert_eq!(a.error_code.as_deref(), Some("TARGET_TIMEOUT"));
    }

    #[test]
    fn check_attempt_proxy_failure_marks_proxy_dead() {
        let mut a = CheckAttempt::new();
        a.fail_proxy(
            ProxyFailureKind::ConnectionRefused,
            "CONNECTION_REFUSED",
            "refused".to_string(),
        );

        assert_eq!(a.outcome(), ProxyCheckOutcome::ProxyFailed);
        assert!(!a.outcome().proxy_is_alive());
        assert!(!a.outcome().fully_working());
        assert!(!a.outcome().is_target_failure());
        assert_eq!(a.error_code.as_deref(), Some("CONNECTION_REFUSED"));
        // Proxy failures do NOT get the TARGET_ prefix.
        assert!(!a.error_code.as_deref().unwrap().starts_with("TARGET_"));
    }

    #[test]
    fn check_attempt_note_warning_keeps_outcome_working() {
        // The HTTP proxy path sets a warning when the HTTP check worked
        // but the HTTPS tunnel through it did not. That must NOT flip
        // the outcome — the proxy is still Working, just not
        // HTTPS-capable.
        let mut a = CheckAttempt::new();
        a.succeed();
        a.note_warning(
            "TARGET_TLS_FAILED",
            "TLS handshake through proxy failed".to_string(),
        );

        assert_eq!(a.outcome(), ProxyCheckOutcome::Working);
        assert!(a.outcome().fully_working());
        assert!(a.failure_kind.is_none(), "warning must not become a failure kind");
        assert_eq!(a.error_code.as_deref(), Some("TARGET_TLS_FAILED"));
    }

    // -- classify_socks_error --

    #[test]
    fn classify_socks_error_maps_timeout() {
        assert_eq!(
            classify_socks_error("timed out reading from proxy"),
            ProxyFailureKind::Timeout
        );
        assert_eq!(
            classify_socks_error("Timed out waiting for response"),
            ProxyFailureKind::Timeout
        );
    }

    #[test]
    fn classify_socks_error_maps_auth_failure() {
        assert_eq!(
            classify_socks_error("SOCKS5 username/password authentication failed"),
            ProxyFailureKind::AuthFailed
        );
    }

    #[test]
    fn classify_socks_error_defaults_to_protocol_failure() {
        assert_eq!(
            classify_socks_error("SOCKS5 connect reply had invalid version"),
            ProxyFailureKind::ProtocolFailure
        );
        assert_eq!(
            classify_socks_error("random thing"),
            ProxyFailureKind::ProtocolFailure
        );
    }

    // -- classify_connect_io_error --

    #[test]
    fn classify_connect_io_error_maps_connection_refused() {
        let err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let (kind, code, _msg) = classify_connect_io_error(&err);
        assert_eq!(kind, ProxyFailureKind::ConnectionRefused);
        assert_eq!(code, "CONNECTION_REFUSED");
    }

    #[test]
    fn classify_connect_io_error_maps_other_to_protocol_failure() {
        let err = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        let (kind, code, _msg) = classify_connect_io_error(&err);
        assert_eq!(kind, ProxyFailureKind::ProtocolFailure);
        assert_eq!(code, "CONNECTION_FAILED");
    }

    // -- proxy_failure_code --

    #[test]
    fn proxy_failure_code_is_scheme_aware_for_protocol_failures() {
        assert_eq!(
            proxy_failure_code(&ProxyScheme::Http, ProxyFailureKind::ProtocolFailure),
            "HTTP_PROXY_FAILED"
        );
        assert_eq!(
            proxy_failure_code(&ProxyScheme::Https, ProxyFailureKind::ProtocolFailure),
            "HTTPS_PROXY_FAILED"
        );
        assert_eq!(
            proxy_failure_code(&ProxyScheme::Socks4, ProxyFailureKind::ProtocolFailure),
            "SOCKS4_FAILED"
        );
        assert_eq!(
            proxy_failure_code(&ProxyScheme::Socks5, ProxyFailureKind::ProtocolFailure),
            "SOCKS5_FAILED"
        );
    }

    #[test]
    fn proxy_failure_code_is_scheme_independent_for_common_kinds() {
        for scheme in [
            ProxyScheme::Http,
            ProxyScheme::Https,
            ProxyScheme::Socks4,
            ProxyScheme::Socks5,
        ] {
            assert_eq!(
                proxy_failure_code(&scheme, ProxyFailureKind::Timeout),
                "CONNECTION_TIMEOUT"
            );
            assert_eq!(
                proxy_failure_code(&scheme, ProxyFailureKind::AuthFailed),
                "AUTH_FAILED"
            );
            assert_eq!(
                proxy_failure_code(&scheme, ProxyFailureKind::ConnectionRefused),
                "CONNECTION_REFUSED"
            );
            assert_eq!(
                proxy_failure_code(&scheme, ProxyFailureKind::TlsFailure),
                "TLS_FAILED"
            );
            assert_eq!(
                proxy_failure_code(&scheme, ProxyFailureKind::DnsFailure),
                "DNS_FAILED"
            );
        }
    }

    // -- verify_http_* — Auth / Target / Proxy distinction --

    /// Spawns a fake TCP server that replies with the given raw bytes
    /// after reading whatever the client sends, and returns its address.
    async fn spawn_fake_http_server(status_line: &'static [u8]) -> std::net::SocketAddr {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(status_line).await;
                let _ = stream.flush().await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn verify_http_proxy_absolute_form_returns_auth_on_407() {
        let addr = spawn_fake_http_server(
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n",
        )
        .await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_proxy_absolute_form(&mut stream, None, Duration::from_secs(2)).await;
        match result {
            Err(VerifyHttpError::Auth { message: msg, .. }) => {
                assert!(msg.contains("407"), "unexpected message: {msg}");
            }
            other => panic!("expected VerifyHttpError::Auth, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_http_proxy_absolute_form_returns_target_on_500() {
        let addr = spawn_fake_http_server(
            b"HTTP/1.1 500 Internal Server Error\r\n\r\n",
        )
        .await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_proxy_absolute_form(&mut stream, None, Duration::from_secs(2)).await;
        match result {
            Err(VerifyHttpError::Target {
                status, message, ..
            }) => {
                assert_eq!(status, 500);
                assert!(message.contains("500"), "unexpected message: {message}");
            }
            other => panic!("expected VerifyHttpError::Target, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_http_proxy_absolute_form_success_reports_status_and_ttfb() {
        let addr = spawn_fake_http_server(b"HTTP/1.1 200 OK\r\n\r\n").await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_proxy_absolute_form(&mut stream, None, Duration::from_secs(2)).await;
        match result {
            Ok(success) => {
                assert_eq!(success.status, 200);
                // TTFB is measured from when read_http_header starts; on
                // localhost it must be tiny but non-negative.
                assert!(success.ttfb.as_nanos() > 0);
            }
            other => panic!("expected Ok(HttpVerifySuccess), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_http_connect_tunnel_returns_auth_on_407() {
        let addr = spawn_fake_http_server(
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n",
        )
        .await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_connect_tunnel(&mut stream, None, Duration::from_secs(2)).await;
        match result {
            Err(VerifyHttpError::Auth { .. }) => {}
            other => panic!("expected VerifyHttpError::Auth, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_http_connect_tunnel_returns_target_on_502() {
        let addr = spawn_fake_http_server(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_connect_tunnel(&mut stream, None, Duration::from_secs(2)).await;
        match result {
            Err(VerifyHttpError::Target { status, .. }) => {
                assert_eq!(status, 502);
            }
            other => panic!("expected VerifyHttpError::Target, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_http_connect_tunnel_returns_proxy_on_bad_status_line() {
        // A response that does not start with "HTTP/" is a protocol
        // failure on the proxy's side, not a target failure.
        let addr = spawn_fake_http_server(b"GARBAGE\r\n\r\n").await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let result =
            verify_http_connect_tunnel(&mut stream, None, Duration::from_secs(2)).await;
        match result {
            Err(VerifyHttpError::Proxy(_)) => {}
            other => panic!("expected VerifyHttpError::Proxy, got {other:?}"),
        }
    }

    // -- Integration: the target-failure-is-not-a-dead-proxy invariant,
    //    via the real build_proxy_result helper. --

    #[test]
    fn build_proxy_result_target_failure_has_working_outcome_and_target_code() {
        let host = ProxyHost::new("8.8.8.8").unwrap();
        let entry = ProxyEntry::new(
            domain::shared::ids::ProxyEntryId::new(),
            domain::shared::ids::ProxyCheckJobId::new(),
            host,
            8080,
            ProxyScheme::Http,
            None,
        )
        .unwrap();

        let mut a = CheckAttempt::new();
        a.proxy_ok = true;
        a.fail_target(
            ProxyFailureKind::Timeout,
            "TIMEOUT",
            "target timed out".to_string(),
        );
        let started = Instant::now();
        let result = build_proxy_result(&entry, started, a);

        // Legacy `reachable` still reflects "fully verified" — false
        // here, because the target did not work.
        assert!(!result.reachable);
        // But the new outcome / helper say the proxy is alive.
        assert_eq!(
            result.effective_outcome(),
            ProxyCheckOutcome::WorkingTargetUnreachable
        );
        assert!(result.is_alive());
        assert!(result.is_target_failure());
        assert_eq!(result.error_code.as_deref(), Some("TARGET_TIMEOUT"));
    }

    #[test]
    fn build_proxy_result_success_has_working_outcome_and_no_failure_kind() {
        let host = ProxyHost::new("8.8.8.8").unwrap();
        let entry = ProxyEntry::new(
            domain::shared::ids::ProxyEntryId::new(),
            domain::shared::ids::ProxyCheckJobId::new(),
            host,
            8080,
            ProxyScheme::Http,
            None,
        )
        .unwrap();

        let mut a = CheckAttempt::new();
        a.succeed();
        a.http_status_code = Some(200);
        a.ttfb_ms = Some(15);
        a.connection_latency_ms = Some(5);
        let started = Instant::now();
        let result = build_proxy_result(&entry, started, a);

        assert!(result.reachable);
        assert_eq!(result.effective_outcome(), ProxyCheckOutcome::Working);
        assert!(result.is_alive());
        assert!(result.is_fully_working());
        assert!(!result.is_target_failure());
        assert_eq!(result.effective_failure_kind(), None);
        assert_eq!(result.http_status_code, Some(200));
        assert_eq!(result.ttfb_ms, Some(15));
        assert_eq!(result.connection_latency_ms, Some(5));
    }


    // ---------------------------------------------------------------
    // PART 3 — integration tests: check_http_proxy end-to-end
    //
    // These drive the real `check_http_proxy` against a local fake TCP
    // server, proving that the target-vs-proxy distinction survives all
    // the way through the pipeline (not just inside CheckAttempt).
    // ---------------------------------------------------------------

    /// Spawns a fake proxy server that replies with `response_bytes` for
    /// each accepted TCP connection, up to `expected_connections` times.
    /// Returns the listener address.
    async fn spawn_fake_proxy_server(
        response_bytes: &'static [u8],
        expected_connections: usize,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..expected_connections {
                let (mut stream, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(response_bytes).await;
                let _ = stream.flush().await;
                // Drop the stream to close it — the client will see EOF
                // on any further reads (e.g. from tls_handshake).
            }
        });
        addr
    }

    fn fake_dest(addr: std::net::SocketAddr) -> crate::ssrf::ResolvedDestination {
        crate::ssrf::ResolvedDestination {
            url: url::Url::parse("http://fake-proxy/").unwrap(),
            host: "fake-proxy".to_string(),
            socket_addr: addr,
        }
    }

    fn make_http_entry() -> ProxyEntry {
        let host = ProxyHost::new("fake-proxy").unwrap();
        ProxyEntry::new(
            domain::shared::ids::ProxyEntryId::new(),
            domain::shared::ids::ProxyCheckJobId::new(),
            host,
            80,
            ProxyScheme::Http,
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn check_http_proxy_reports_target_failure_on_500_response() {
        // THE central PART 3 scenario, proved end-to-end: the fake proxy
        // speaks HTTP correctly but returns 500. The proxy itself is
        // alive; only the target through it failed.
        let addr =
            spawn_fake_proxy_server(b"HTTP/1.1 500 Internal Server Error\r\n\r\n", 1).await;
        let dest = fake_dest(addr);
        let entry = make_http_entry();

        let attempt = check_http_proxy(
            &entry,
            &dest,
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .await;

        assert!(
            attempt.proxy_ok,
            "a well-formed HTTP response means the proxy is alive, even if the status is 500"
        );
        assert!(!attempt.target_ok);
        assert_eq!(
            attempt.outcome(),
            ProxyCheckOutcome::WorkingTargetUnreachable
        );
        assert_eq!(attempt.http_status_code, Some(500));
        assert_eq!(
            attempt.failure_kind,
            Some(ProxyFailureKind::TargetHttpError)
        );
        assert_eq!(attempt.error_code.as_deref(), Some("TARGET_HTTP_ERROR"));
        assert!(
            attempt.connection_latency_ms.is_some(),
            "connection latency must be measured"
        );
        assert!(attempt.ttfb_ms.is_some(), "ttfb must be measured");
    }

    #[tokio::test]
    async fn check_http_proxy_reports_proxy_failure_on_connection_refused() {
        // Bind a port, capture its address, then drop the listener so
        // that any subsequent connect is refused by the OS.
        let listener =
            TcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .await
                .unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let dest = fake_dest(addr);
        let entry = make_http_entry();

        let attempt = check_http_proxy(
            &entry,
            &dest,
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .await;

        assert!(!attempt.proxy_ok);
        assert!(!attempt.target_ok);
        assert_eq!(attempt.outcome(), ProxyCheckOutcome::ProxyFailed);
        assert_eq!(
            attempt.failure_kind,
            Some(ProxyFailureKind::ConnectionRefused)
        );
        assert_eq!(
            attempt.error_code.as_deref(),
            Some("CONNECTION_REFUSED")
        );
        // Connection latency is still recorded — we measured the
        // attempt, even though it failed.
        assert!(attempt.connection_latency_ms.is_some());
    }

    #[tokio::test]
    async fn check_http_proxy_reports_auth_failure_on_407() {
        let addr = spawn_fake_proxy_server(
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n",
            1,
        )
        .await;
        let dest = fake_dest(addr);
        let entry = make_http_entry();

        let attempt = check_http_proxy(
            &entry,
            &dest,
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .await;

        assert!(
            !attempt.proxy_ok,
            "407 is an authentication failure — a proxy-side failure"
        );
        assert_eq!(attempt.outcome(), ProxyCheckOutcome::ProxyFailed);
        assert_eq!(attempt.failure_kind, Some(ProxyFailureKind::AuthFailed));
        assert_eq!(attempt.error_code.as_deref(), Some("AUTH_FAILED"));
    }

    #[tokio::test]
    async fn check_http_proxy_marks_working_when_200_and_tls_tunnel_fails() {
        // The fake server will accept TWO connections: one for the
        // absolute-form HTTP GET (reply 200), one for the CONNECT tunnel
        // (reply 200). The subsequent TLS handshake will fail because
        // our fake server does not speak TLS — that must produce a
        // WARNING on an otherwise-Working attempt, not flip the outcome.
        let addr = spawn_fake_proxy_server(b"HTTP/1.1 200 OK\r\n\r\n", 2).await;
        let dest = fake_dest(addr);
        let entry = make_http_entry();

        let attempt = check_http_proxy(
            &entry,
            &dest,
            Duration::from_secs(2),
            Duration::from_secs(2),
        )
        .await;

        assert!(
            attempt.proxy_ok,
            "200 on the absolute-form GET means the proxy is alive"
        );
        assert!(
            attempt.target_ok,
            "the target through the proxy returned a valid HTTP response"
        );
        assert_eq!(attempt.outcome(), ProxyCheckOutcome::Working);
        assert_eq!(attempt.http_status_code, Some(200));
        assert!(
            !attempt.supports_https,
            "TLS handshake over our fake stream must fail"
        );
        assert_eq!(
            attempt.error_code.as_deref(),
            Some("TARGET_TLS_FAILED")
        );
        assert!(
            attempt.failure_kind.is_none(),
            "a warning must not set a failure_kind"
        );
        // Both TTFB (from the HTTP GET) and connection latency are set.
        assert!(attempt.ttfb_ms.is_some());
        assert!(attempt.connection_latency_ms.is_some());
    }

}
