//! Diagnostic checks run immediately after a monitor's main check fails,
//! so the customer sees WHY it went down (DNS / TCP / TLS / HTTP) instead
//! of just "Down".
//!
//! ## SSRF / DNS-rebinding note
//!
//! This module NEVER performs a second, independent DNS lookup for a
//! destination the main check already resolved+validated. When the main
//! check succeeded in resolving+validating the monitor's host but failed
//! at a LATER stage (TCP/TLS/HTTP), the caller passes that exact
//! `shared::ssrf::ResolvedDestination` in as `existing_destination`, and
//! Step 1 here reuses its already-validated IP verbatim — resolving again
//! at this point could return a *different* IP (DNS rebinding) and would
//! defeat the whole point of pinning.
//!
//! The ONE exception is when the main check itself failed to resolve or
//! SSRF-validate the destination in the first place (`existing_destination
//! == None`): in that case there is no IP to reuse, and Step 1's own
//! attempt via the SAME SSRF-guarded resolver *is* the diagnosis ("DNS
//! resolution failed"). This never gives an attacker two independent
//! chances at two different addresses — if it fails, every later step is
//! `Skipped`; if it unexpectedly succeeds, that resolved IP is used
//! immediately and only within this same diagnostic run (Steps 2-4),
//! never handed back out.
//!
//! No diagnostic step here ever dials anything other than the monitor's
//! own already-validated destination — no extra attack surface is
//! introduced beyond what the main check already reaches.

use std::time::{Duration, Instant};

use domain::{DiagnosticStep, DiagnosticStepStatus, Monitor};
use shared::ssrf::ResolvedDestination;

use crate::tls_cert;

/// Hard cap on the total wall-clock time a single diagnostic run may
/// take, regardless of how many steps run or how slow the target is at
/// each stage. A diagnostic run must never become a second, heavier check
/// piled on top of every single failure.
pub const DIAGNOSTIC_TOTAL_TIMEOUT: Duration = Duration::from_secs(15);

/// Per-step timeout budget, kept well under `DIAGNOSTIC_TOTAL_TIMEOUT` so
/// the outer timeout is the one that ultimately bounds the run.
const STEP_TIMEOUT: Duration = Duration::from_secs(5);

/// Result of a diagnostic run: the step-by-step timeline plus a single
/// template-generated root cause summary.
pub struct DiagnosticOutcome {
    pub steps: Vec<DiagnosticStep>,
    pub root_cause_summary: String,
}

/// Runs the diagnostic step chain for `monitor` immediately after its
/// main check has failed, while the monitor is still "warm" in memory
/// (called synchronously from `checker.rs::check_monitor` on the same
/// failure path — no waiting for the next scheduled check).
///
/// See the module-level SSRF note above for the contract on
/// `existing_destination`.
pub async fn run_diagnostics(
    monitor: &Monitor,
    existing_destination: Option<&ResolvedDestination>,
) -> DiagnosticOutcome {
    match tokio::time::timeout(
        DIAGNOSTIC_TOTAL_TIMEOUT,
        run_diagnostics_inner(monitor, existing_destination),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_elapsed) => DiagnosticOutcome {
            steps: vec![DiagnosticStep::new(
                "Diagnostics",
                DiagnosticStepStatus::Failed,
                DIAGNOSTIC_TOTAL_TIMEOUT.as_millis() as u64,
                "Diagnostic run exceeded its 15 second time budget",
            )],
            root_cause_summary: "Diagnostics timed out before a root cause could be determined"
                .to_string(),
        },
    }
}

async fn run_diagnostics_inner(
    monitor: &Monitor,
    existing_destination: Option<&ResolvedDestination>,
) -> DiagnosticOutcome {
    let mut steps: Vec<DiagnosticStep> = Vec::with_capacity(4);

    // ---- Step 1: DNS resolution ----
    let destination: Option<ResolvedDestination> = match existing_destination {
        Some(existing) => {
            steps.push(DiagnosticStep::new(
                "DNS resolution",
                DiagnosticStepStatus::Success,
                0,
                format!(
                    "Already resolved to {} by the primary check; reused to avoid a second DNS lookup",
                    existing.socket_addr.ip()
                ),
            ));
            Some(existing.clone())
        }
        None => {
            let started = Instant::now();
            match shared::ssrf::resolve_and_validate_destination(monitor.url.as_str()).await {
                Ok(resolved) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    steps.push(DiagnosticStep::new(
                        "DNS resolution",
                        DiagnosticStepStatus::Success,
                        elapsed,
                        format!("Resolved to {} in {elapsed}ms", resolved.socket_addr.ip()),
                    ));
                    Some(resolved)
                }
                Err(_err) => {
                    let elapsed = started.elapsed().as_millis() as u64;
                    steps.push(DiagnosticStep::new(
                        "DNS resolution",
                        DiagnosticStepStatus::Failed,
                        elapsed,
                        format!(
                            "Failed to resolve or validate the monitor's hostname after {elapsed}ms"
                        ),
                    ));
                    None
                }
            }
        }
    };

    let Some(destination) = destination else {
        let dns_ms = steps[0].duration_ms;
        steps.push(DiagnosticStep::skipped("TCP connect"));
        steps.push(DiagnosticStep::skipped("TLS handshake"));
        steps.push(DiagnosticStep::skipped("HTTP response"));

        return DiagnosticOutcome {
            root_cause_summary: format!("DNS resolution failed after {dns_ms}ms"),
            steps,
        };
    };

    let is_https = monitor.url.as_str().starts_with("https://");

    // ---- Step 2: TCP connect ----
    let started = Instant::now();
    let tcp_result = tokio::time::timeout(
        STEP_TIMEOUT,
        tokio::net::TcpStream::connect(destination.socket_addr),
    )
    .await;
    let tcp_elapsed = started.elapsed().as_millis() as u64;

    let tcp_failure_reason: Option<&'static str> = match tcp_result {
        Ok(Ok(_stream)) => {
            steps.push(DiagnosticStep::new(
                "TCP connect",
                DiagnosticStepStatus::Success,
                tcp_elapsed,
                format!(
                    "Connected to {} in {tcp_elapsed}ms",
                    destination.socket_addr
                ),
            ));
            None
        }
        Ok(Err(err)) => {
            let reason = classify_connect_error(&err);
            steps.push(DiagnosticStep::new(
                "TCP connect",
                DiagnosticStepStatus::Failed,
                tcp_elapsed,
                format!(
                    "Failed to connect to {} after {tcp_elapsed}ms: {reason}",
                    destination.socket_addr
                ),
            ));
            Some(reason)
        }
        Err(_timed_out) => {
            steps.push(DiagnosticStep::new(
                "TCP connect",
                DiagnosticStepStatus::Failed,
                tcp_elapsed,
                format!(
                    "Connection attempt to {} timed out after {tcp_elapsed}ms",
                    destination.socket_addr
                ),
            ));
            Some("connection timeout")
        }
    };

    if let Some(reason) = tcp_failure_reason {
        if is_https {
            steps.push(DiagnosticStep::skipped("TLS handshake"));
        }
        steps.push(DiagnosticStep::skipped("HTTP response"));

        return DiagnosticOutcome {
            root_cause_summary: format!("TCP connection failed after {tcp_elapsed}ms ({reason})"),
            steps,
        };
    }

    // ---- Step 3: TLS handshake (only for https) ----
    if is_https {
        let started = Instant::now();
        let handshake =
            tls_cert::probe_tls_handshake(&destination.host, destination.socket_addr, STEP_TIMEOUT)
                .await;
        let tls_elapsed = started.elapsed().as_millis() as u64;

        if handshake.is_err() {
            let expired_on = tls_cert::probe_expired_cert_date(
                &destination.host,
                destination.socket_addr,
                STEP_TIMEOUT,
            )
            .await;

            let detail = match expired_on {
                Some(expires_at) => {
                    format!("Certificate expired on {}", expires_at.format("%Y-%m-%d"))
                }
                None => format!("TLS handshake failed after {tls_elapsed}ms"),
            };

            steps.push(DiagnosticStep::new(
                "TLS handshake",
                DiagnosticStepStatus::Failed,
                tls_elapsed,
                detail.clone(),
            ));
            steps.push(DiagnosticStep::skipped("HTTP response"));

            return DiagnosticOutcome {
                root_cause_summary: format!("TLS handshake failed: {detail}"),
                steps,
            };
        }

        steps.push(DiagnosticStep::new(
            "TLS handshake",
            DiagnosticStepStatus::Success,
            tls_elapsed,
            format!("TLS handshake completed in {tls_elapsed}ms"),
        ));
    }

    // ---- Step 4: HTTP request/response ----
    let started = Instant::now();
    let http_result = probe_http(monitor, &destination).await;
    let http_elapsed = started.elapsed().as_millis() as u64;

    match http_result {
        Ok(status) => {
            steps.push(DiagnosticStep::new(
                "HTTP response",
                DiagnosticStepStatus::Success,
                http_elapsed,
                format!("Received HTTP {status} in {http_elapsed}ms"),
            ));

            DiagnosticOutcome {
                root_cause_summary: format!(
                    "Server responded with HTTP {status}, which did not match what the monitor expected"
                ),
                steps,
            }
        }
        Err(_err) => {
            steps.push(DiagnosticStep::new(
                "HTTP response",
                DiagnosticStepStatus::Failed,
                http_elapsed,
                format!("No valid HTTP response received after {http_elapsed}ms"),
            ));

            DiagnosticOutcome {
                root_cause_summary: format!(
                    "Server accepted the connection but did not return a valid HTTP response within {http_elapsed}ms"
                ),
                steps,
            }
        }
    }
}

/// Classifies a TCP connect `io::Error` into a small, fixed set of
/// template phrases — never the raw OS error text — so the phrase can
/// safely flow into `root_cause_summary`.
fn classify_connect_error(err: &std::io::Error) -> &'static str {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::ConnectionRefused => "connection refused",
        ErrorKind::TimedOut => "connection timeout",
        _ => "connection failed",
    }
}

/// Sends a single diagnostic HTTP request against the already-resolved,
/// already-SSRF-validated `destination` — using the exact same
/// `.resolve(host, socket_addr)` DNS-pinning pattern as
/// `checker.rs::check_monitor`, so this never triggers a second DNS
/// lookup either. Redirects are disabled for the same SSRF reason as the
/// main checker.
async fn probe_http(
    monitor: &Monitor,
    destination: &ResolvedDestination,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let method = match monitor.method {
        domain::HttpMethod::Get => reqwest::Method::GET,
        domain::HttpMethod::Head => reqwest::Method::HEAD,
    };

    let pinned_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .build()?;

    let response = pinned_client
        .request(method, destination.url.clone())
        .timeout(STEP_TIMEOUT)
        .send()
        .await?;

    Ok(response.status().as_u16())
}
