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
use base64::Engine;
use domain::ProxyScheme;
use domain::entities::{ProxyCheckJob, ProxyEntry};
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
        ProxyScheme::Socks5 => "socks5",
    }
}

fn elapsed_ms(started: Instant) -> u32 {
    started.elapsed().as_millis().try_into().unwrap_or(u32::MAX)
}

fn proxy_result(
    started: Instant,
    reachable: bool,
    supports_https: bool,
    error: Option<String>,
) -> ProxyResult {
    ProxyResult {
        reachable,
        latency_ms: Some(elapsed_ms(started)),
        anonymity: None,
        supports_https,
        country_code: None,
        error,
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

async fn read_http_header<S>(stream: &mut S, timeout_duration: Duration) -> Result<String, String>
where
    S: AsyncRead + Unpin,
{
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        if buf.len() > MAX_PROXY_RESPONSE_HEADER_BYTES {
            return Err("proxy response headers exceeded size limit".to_string());
        }

        let n = timeout(timeout_duration, stream.read(&mut chunk))
            .await
            .map_err(|_| "timed out waiting for proxy response".to_string())?
            .map_err(|e| format!("failed to read proxy response: {e}"))?;

        if n == 0 {
            return Err("proxy closed connection before completing response headers".to_string());
        }

        buf.extend_from_slice(&chunk[..n]);

        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(buf).map_err(|_| "proxy response was not valid UTF-8".to_string())
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

async fn verify_http_proxy_absolute_form<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "GET http://{HTTPS_VERIFY_TARGET_HOST}/ HTTP/1.1\r\nHost: {HTTPS_VERIFY_TARGET_HOST}\r\nUser-Agent: PingVertex/1.0\r\nConnection: close\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration).await?;
    let header = read_http_header(stream, timeout_duration).await?;
    let status = parse_status_code(&header)?;

    if !(200..400).contains(&status) {
        return Err(format!("proxy returned HTTP status {status}"));
    }

    Ok(())
}

async fn verify_http_connect_tunnel<S>(
    stream: &mut S,
    creds: Option<&ProxyCredentials>,
    timeout_duration: Duration,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let target = format!("{HTTPS_VERIFY_TARGET_HOST}:{HTTPS_VERIFY_TARGET_PORT}");
    let auth = proxy_authorization_header(creds);
    let request = format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nUser-Agent: PingVertex/1.0\r\nProxy-Connection: keep-alive\r\n{auth}\r\n\r\n"
    );

    write_all_timeout(stream, request.as_bytes(), timeout_duration).await?;
    let header = read_http_header(stream, timeout_duration).await?;
    let status = parse_status_code(&header)?;

    if !(200..300).contains(&status) {
        return Err(format!("proxy returned CONNECT status {status}"));
    }

    Ok(())
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
async fn check_single_entry(entry: &ProxyEntry) -> ProxyResult {
    let target = format!(
        "{}://{}:{}",
        proxy_scheme_str(entry.scheme()),
        entry.host().as_str(),
        entry.port()
    );

    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_destination(&target).await {
        Ok(destination) => destination,
        Err(err) => return proxy_result(started, false, false, Some(err)),
    };

    let connect_timeout = Duration::from_secs(PER_ENTRY_TIMEOUT_SECS);
    let protocol_timeout = Duration::from_secs(PROTOCOL_TIMEOUT_SECS);

    let (reachable, supports_https, error) = match entry.scheme() {
        ProxyScheme::Http => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(mut stream)) => {
                    match verify_http_proxy_absolute_form(
                        &mut stream,
                        entry.credentials(),
                        protocol_timeout,
                    )
                    .await
                    {
                        Ok(()) => (true, false, None),
                        Err(err) => (false, false, Some(err)),
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
        ProxyScheme::Https => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(tcp)) => {
                    let connector = match crate::proxy_tls::tls_connector() {
                        Ok(c) => c,
                        Err(err) => {
                            return proxy_result(started, false, false, Some(err.to_string()));
                        }
                    };
                    let server_name = match ServerName::try_from(entry.host().as_str().to_string())
                    {
                        Ok(name) => name,
                        Err(err) => {
                            return proxy_result(started, false, false, Some(err.to_string()));
                        }
                    };

                    match timeout(protocol_timeout, connector.connect(server_name, tcp)).await {
                        Ok(Ok(mut proxy_tls)) => {
                            if let Err(err) = verify_http_connect_tunnel(
                                &mut proxy_tls,
                                entry.credentials(),
                                protocol_timeout,
                            )
                            .await
                            {
                                (false, false, Some(err))
                            } else if let Err(err) =
                                tls_handshake(proxy_tls, HTTPS_VERIFY_TARGET_HOST, protocol_timeout)
                                    .await
                            {
                                (false, false, Some(err))
                            } else {
                                (true, true, None)
                            }
                        }
                        Ok(Err(err)) => (false, false, Some(format!("TLS to proxy failed: {err}"))),
                        Err(_) => (
                            false,
                            false,
                            Some("TLS handshake with proxy timed out".to_string()),
                        ),
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
        ProxyScheme::Socks5 => {
            match timeout(connect_timeout, TcpStream::connect(destination.socket_addr)).await {
                Ok(Ok(mut stream)) => {
                    if let Err(err) = verify_socks5_proxy(
                        &mut stream,
                        entry.credentials(),
                        HTTPS_VERIFY_TARGET_HOST,
                        HTTPS_VERIFY_TARGET_PORT,
                        protocol_timeout,
                    )
                    .await
                    {
                        (false, false, Some(err))
                    } else if let Err(err) =
                        tls_handshake(stream, HTTPS_VERIFY_TARGET_HOST, protocol_timeout).await
                    {
                        (
                            true,
                            false,
                            Some(format!(
                                "SOCKS5 tunnel connected but TLS to target failed: {err}"
                            )),
                        )
                    } else {
                        (true, true, None)
                    }
                }
                Ok(Err(err)) => (false, false, Some(err.to_string())),
                Err(_) => (
                    false,
                    false,
                    Some(format!(
                        "connection timed out after {PER_ENTRY_TIMEOUT_SECS}s"
                    )),
                ),
            }
        }
    };

    proxy_result(started, reachable, supports_https, error)
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
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: ProxyCheckJobRepository + ?Sized,
{
    if !job.mark_running() {
        // Another worker already observed this job as non-Pending and
        // transitioned it first; this call is a no-op by design (see
        // `ProxyCheckJob::mark_running`).
        info!(job_id = %job.id(), "proxy check job already running or finished; skipping");
        return Ok(());
    }

    // This state transition is important enough to propagate a failure
    // for: if it can't even be persisted, the job's status in storage
    // would silently disagree with what this worker believes, so the
    // caller should treat this as fatal for this job rather than
    // continuing to check entries against a job the database still shows
    // as `Pending`.
    repository.update_job(&job).await?;

    info!(
        job_id = %job.id(),
        entry_count = entries.len(),
        "starting proxy check job"
    );

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS_PER_JOB));
    let mut tasks = JoinSet::new();

    for entry in entries {
        let semaphore = Arc::clone(&semaphore);
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("semaphore is never closed while this job is running");

            let mut entry = entry;
            let result = check_single_entry(&entry).await;
            entry.set_result(result);
            entry
        });
    }

    let mut checked_count: u32 = 0;

    while let Some(join_result) = tasks.join_next().await {
        match join_result {
            Ok(entry) => {
                checked_count += 1;

                if let Err(err) = repository.update_entry(&entry).await {
                    // A single entry failing to persist must not abort
                    // the rest of the job — log and continue, mirroring
                    // how `scheduler.rs` treats a failed `Check` save as
                    // non-fatal to the overall sweep.
                    error!(
                        job_id = %job.id(),
                        entry_id = %entry.id(),
                        error = %err,
                        "failed to persist proxy entry result"
                    );
                }

                job.record_progress(checked_count);

                if let Err(err) = repository.update_job(&job).await {
                    error!(
                        job_id = %job.id(),
                        error = %err,
                        "failed to persist proxy check job progress"
                    );
                }
            }
            Err(join_err) => {
                // The checking task itself panicked. The entry it was
                // checking never got a result recorded, but the job as a
                // whole must still be able to reach `Completed` once
                // every task has resolved (panicked or not) — counting it
                // here keeps `checked_count` converging on
                // `entries.len()` rather than stalling short of it.
                checked_count += 1;
                job.record_progress(checked_count);

                warn!(
                    job_id = %job.id(),
                    error = %join_err,
                    "proxy entry check task panicked"
                );
            }
        }
    }

    job.mark_completed();

    if let Err(err) = repository.update_job(&job).await {
        error!(
            job_id = %job.id(),
            error = %err,
            "failed to persist proxy check job completion"
        );
        return Err(err);
    }

    info!(job_id = %job.id(), "proxy check job completed");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::IdGenerator;
    use domain::shared::ids::{TenantId, UserId};
    use domain::value_objects::ProxyHost;
    use infra::{InMemoryProxyCheckJobRepository, UuidIdGenerator};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

        let entries = repo.find_entries_by_job_id(job_id).await.unwrap();

        run_proxy_check_job(&repo, job.clone(), entries)
            .await
            .unwrap();

        let job_after = repo.find_job_by_id(job_id).await.unwrap().unwrap();
        assert_eq!(job_after.status(), domain::ProxyCheckJobStatus::Completed);
        assert_eq!(job_after.checked_count(), 1);

        let entries_after = repo.find_entries_by_job_id(job_id).await.unwrap();
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

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("size limit"));
    }
}
