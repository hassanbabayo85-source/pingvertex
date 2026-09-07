use std::{
    collections::HashMap,
    error::Error as StdError,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use domain::{CheckErrorKind, CheckResult, HttpMethod, Monitor, RegionCode};
use reqwest::Client;
use shared::ssrf::ResolvedDestination;
use tracing::warn;

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};
use crate::diagnostics::run_diagnostics;

type Result = std::result::Result<CheckResult, Box<dyn StdError + Send + Sync>>;

/// Maximum number of response-body bytes an HTTP check will ever read.
/// The checker never reads an unbounded response body; if a response is
/// larger than this, the check fails with `CheckErrorKind::HttpError`.
const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 1024 * 1024;

/// The result of an HTTP/HTTPS check execution plus its canonical error
/// classification. `CheckErrorKind` is the shared taxonomy from B18 and
/// is the field every active check worker must populate.
#[derive(Debug, Clone)]
pub struct HttpCheckOutcome {
    pub result: CheckResult,
    pub error_kind: CheckErrorKind,
    pub attempted: bool,
}

impl HttpCheckOutcome {
    fn new(result: CheckResult, error_kind: CheckErrorKind) -> Self {
        Self {
            result,
            error_kind,
            attempted: true,
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
            attempted: false,
        }
    }

    fn failure_attempted(
        region: RegionCode,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self {
            result: CheckResult::failure(region, latency_ms, error),
            error_kind,
            attempted: true,
        }
    }

    fn failure_with_status(
        region: RegionCode,
        status: u16,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self {
            result: CheckResult::failure_with_status(region, status, latency_ms, error),
            error_kind,
            attempted: true,
        }
    }
}

/// Fetches TLS certificate expiry info for `monitor`'s endpoint, if
/// `monitor.ssl_check_enabled` is set and `monitor.url` is `https://`.
/// Otherwise returns `(None, None)` immediately without any network I/O.
///
/// `hostname` and `socket_addr` MUST already have been through SSRF
/// validation by the caller.
pub async fn maybe_fetch_cert_info(
    monitor: &Monitor,
    hostname: &str,
    socket_addr: SocketAddr,
) -> (Option<DateTime<Utc>>, Option<i64>) {
    if !monitor.ssl_check_enabled || !monitor.url.as_str().starts_with("https://") {
        return (None, None);
    }

    let timeout_duration = Duration::from_secs(monitor.timeout_seconds.into());

    match crate::tls_cert::fetch_certificate_info(hostname, socket_addr, timeout_duration).await {
        Ok(info) => {
            if info.days_remaining <= monitor.ssl_expiry_warning_days as i64 {
                warn!(
                    monitor_id = %monitor.id.0,
                    name = %monitor.name.as_str(),
                    days_remaining = info.days_remaining,
                    warning_threshold_days = monitor.ssl_expiry_warning_days,
                    "TLS certificate is expiring soon"
                );
            }

            (Some(info.expires_at), Some(info.days_remaining))
        }
        Err(err) => {
            warn!(
                monitor_id = %monitor.id.0,
                name = %monitor.name.as_str(),
                error = %err,
                "failed to fetch TLS certificate info; continuing without cert data"
            );

            (None, None)
        }
    }
}

/// Performs a real HTTP/HTTPS check with SSRF-safe DNS pinning and a
/// total-timeout budget. This is the production path used by
/// `HttpCheckExecutor`.
pub async fn check_http(monitor: &Monitor) -> HttpCheckOutcome {
    if !monitor.enabled {
        return HttpCheckOutcome::failure(
            RegionCode::default_region(),
            0,
            CheckErrorKind::Unknown,
            "monitor is disabled",
        );
    }

    let timeout_duration = Duration::from_secs(monitor.timeout_seconds.into());
    let overall_started = Instant::now();
    let attempt_started = Arc::new(AtomicBool::new(false));

    match tokio::time::timeout(
        timeout_duration,
        check_http_real(monitor, Arc::clone(&attempt_started)),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            let mut outcome = HttpCheckOutcome::failure(
                RegionCode::default_region(),
                overall_started.elapsed().as_millis(),
                CheckErrorKind::Timeout,
                "HTTP check timed out",
            );
            outcome.attempted = attempt_started.load(Ordering::Acquire);

            let diag = run_diagnostics(monitor, None, CheckErrorKind::Timeout).await;
            outcome.result = outcome
                .result
                .with_diagnostics(diag.steps, diag.root_cause_summary);
            outcome
        }
    }
}

/// SSRF-safe, DNS-pinned HTTP/HTTPS check body. The caller wraps this in
/// `tokio::time::timeout` so DNS + TCP + TLS + response are all bounded
/// by the monitor's single timeout budget.
async fn check_http_real(monitor: &Monitor, attempt_started: Arc<AtomicBool>) -> HttpCheckOutcome {
    let started = Instant::now();

    let destination = match crate::ssrf::resolve_and_validate_destination(&monitor.url).await {
        Ok(destination) => destination,
        Err(err) => {
            let latency_ms = started.elapsed().as_millis();
            let mut outcome = HttpCheckOutcome::failure(
                RegionCode::default_region(),
                latency_ms,
                CheckErrorKind::DnsError,
                err,
            );

            let diag = run_diagnostics(monitor, None, CheckErrorKind::DnsError).await;
            outcome.result = outcome
                .result
                .with_diagnostics(diag.steps, diag.root_cause_summary);
            return outcome;
        }
    };

    let pinned_client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        // SECURITY: without this, reqwest honors HTTP_PROXY/HTTPS_PROXY/
        // ALL_PROXY from the process environment and hands the request's
        // destination host+port to that proxy verbatim — bypassing the
        // `.resolve()` DNS pinning above entirely, since the proxy (not
        // this process) does its own DNS resolution. That reopens the
        // exact SSRF/DNS-rebinding hole `resolve_and_validate_destination`
        // exists to close: a monitor target could resolve to a public IP
        // at validation time and a private one at proxy-connect time.
        .no_proxy()
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            let latency_ms = started.elapsed().as_millis();
            let mut outcome = HttpCheckOutcome::failure(
                RegionCode::default_region(),
                latency_ms,
                CheckErrorKind::InternalError,
                err.to_string(),
            );

            let diag =
                run_diagnostics(monitor, Some(&destination), CheckErrorKind::InternalError).await;
            outcome.result = outcome
                .result
                .with_diagnostics(diag.steps, diag.root_cause_summary);
            return outcome;
        }
    };

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let request_started = Instant::now();
    attempt_started.store(true, Ordering::Release);
    let response_result = pinned_client
        .request(method, destination.url.clone())
        .send()
        .await;

    let mut outcome = handle_http_response(
        response_result,
        request_started.elapsed().as_millis(),
        monitor,
        true,
        Some(&destination),
    )
    .await;

    if !outcome.result.success {
        let failure_kind = outcome.error_kind;
        let diag = run_diagnostics(monitor, Some(&destination), failure_kind).await;
        outcome.result = outcome
            .result
            .with_diagnostics(diag.steps, diag.root_cause_summary);
    }

    outcome
}

/// Backwards-compatible HTTP check entry point used by the legacy
/// sweep scheduler (`scheduler.rs`).
pub async fn check_monitor(_client: &Client, monitor: &Monitor) -> Result {
    Ok(check_http(monitor).await.result)
}

/// Test-friendly HTTP check that performs the actual HTTP/HTTPS request,
/// but uses an injected destination validator instead of the real SSRF
/// resolver. The response is read with the same bounded-body guard as the
/// production path.
pub async fn check_http_with_validator<F, Fut>(
    client: &Client,
    monitor: &Monitor,
    validate_destination: F,
) -> std::result::Result<HttpCheckOutcome, Box<dyn StdError + Send + Sync>>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), Box<dyn StdError + Send + Sync>>>,
{
    if !monitor.enabled {
        return Ok(HttpCheckOutcome::failure(
            RegionCode::default_region(),
            0,
            CheckErrorKind::Unknown,
            "monitor is disabled",
        ));
    }

    validate_destination(monitor.url.as_str().to_string()).await?;

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let started = Instant::now();
    let response_result = client
        .request(method, monitor.url.as_str())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    Ok(handle_http_response(
        response_result,
        started.elapsed().as_millis(),
        monitor,
        true,
        None,
    )
    .await)
}

/// Backwards-compatible validator path used by existing tests in
/// `crates/worker/tests/checker.rs`. Unlike `check_http_with_validator`,
/// an unexpected HTTP status is represented without a `status_code` on the
/// legacy `CheckResult` (preserving pre-B18 test expectations). All other
/// behavior, including bounded body reading, is identical.
pub async fn check_monitor_with_validator<F, Fut>(
    client: &Client,
    monitor: &Monitor,
    validate_destination: F,
) -> Result
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), Box<dyn StdError + Send + Sync>>>,
{
    if !monitor.enabled {
        return Ok(CheckResult::failure(
            RegionCode::default_region(),
            0,
            "monitor is disabled",
        ));
    }

    validate_destination(monitor.url.as_str().to_string()).await?;

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let started = Instant::now();
    let response_result = client
        .request(method, monitor.url.as_str())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    Ok(handle_http_response(
        response_result,
        started.elapsed().as_millis(),
        monitor,
        false,
        None,
    )
    .await
    .result)
}

/// Shared response evaluation for both the production and validator paths.
///
/// `include_status_on_mismatch` controls whether a wrong HTTP status is
/// persisted on `CheckResult.status_code`. The B18 production path always
/// captures the actual status (even when the check fails), while the legacy
/// validator path keeps its historical shape for existing tests.
async fn handle_http_response(
    response_result: reqwest::Result<reqwest::Response>,
    latency_ms: u128,
    monitor: &Monitor,
    include_status_on_mismatch: bool,
    destination: Option<&ResolvedDestination>,
) -> HttpCheckOutcome {
    match response_result {
        Ok(response) => {
            let status = response.status().as_u16();

            if status != monitor.expected_status {
                let error_msg = format!("expected {}, got {}", monitor.expected_status, status);
                let result = if include_status_on_mismatch {
                    CheckResult::failure_with_status(
                        RegionCode::default_region(),
                        status,
                        latency_ms,
                        error_msg,
                    )
                } else {
                    CheckResult::failure(RegionCode::default_region(), latency_ms, error_msg)
                };

                return HttpCheckOutcome::new(result, CheckErrorKind::HttpError);
            }

            let (headers, body_bytes) = if monitor.assertions.is_empty() {
                (HashMap::new(), Vec::new())
            } else {
                let headers = collect_headers(&response);
                match read_body_bounded(response).await {
                    Ok(body) => (headers, body),
                    Err(error_kind) => {
                        return HttpCheckOutcome::failure_with_status(
                            RegionCode::default_region(),
                            status,
                            latency_ms,
                            error_kind,
                            "HTTP response body exceeded the maximum permitted size",
                        );
                    }
                }
            };

            if !monitor.assertions.is_empty() {
                let ctx = EvalContext {
                    status_code: status,
                    headers,
                    body_bytes,
                    latency_ms,
                };

                let (all_passed, results) = evaluate_assertions(&monitor.assertions, &ctx);

                if !all_passed {
                    let errors: Vec<String> =
                        results.iter().filter_map(|r| r.message.clone()).collect();
                    let error_msg = format!("assertions sun gaza: {}", errors.join("; "));

                    return HttpCheckOutcome::failure_with_status(
                        RegionCode::default_region(),
                        status,
                        latency_ms,
                        CheckErrorKind::AssertionFailed,
                        error_msg,
                    );
                }
            }

            let (cert_expires_at, cert_days_remaining) = match destination {
                Some(destination) => {
                    maybe_fetch_cert_info(monitor, &destination.host, destination.socket_addr).await
                }
                None => (None, None),
            };

            HttpCheckOutcome::new(
                CheckResult::success(RegionCode::default_region(), status, latency_ms)
                    .with_cert_info(cert_expires_at, cert_days_remaining),
                CheckErrorKind::Unknown,
            )
        }

        Err(err) => {
            let error_kind = classify_reqwest_error(&err);
            HttpCheckOutcome::failure(
                RegionCode::default_region(),
                latency_ms,
                error_kind,
                err.to_string(),
            )
        }
    }
}

fn collect_headers(response: &reqwest::Response) -> HashMap<String, String> {
    response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// Reads the response body up to `MAX_HTTP_RESPONSE_BODY_BYTES`. Never
/// reads unbounded and never accepts more than the configured cap.
async fn read_body_bounded(
    mut response: reqwest::Response,
) -> std::result::Result<Vec<u8>, CheckErrorKind> {
    let mut body = Vec::new();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > MAX_HTTP_RESPONSE_BODY_BYTES {
                    return Err(CheckErrorKind::HttpError);
                }

                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(err) => return Err(classify_reqwest_error(&err)),
        }
    }

    Ok(body)
}

fn classify_reqwest_error(err: &reqwest::Error) -> CheckErrorKind {
    if err.is_timeout() {
        CheckErrorKind::Timeout
    } else if is_tls_error(err) {
        CheckErrorKind::TlsError
    } else if err.is_connect() {
        CheckErrorKind::ConnectionError
    } else if err.is_body() {
        CheckErrorKind::HttpError
    } else {
        CheckErrorKind::Unknown
    }
}

fn is_tls_error(err: &reqwest::Error) -> bool {
    let mut source = StdError::source(err);

    while let Some(err) = source {
        let msg = err.to_string();

        if msg.contains("tls")
            || msg.contains("TLS")
            || msg.contains("ssl")
            || msg.contains("SSL")
            || msg.contains("certificate")
        {
            return true;
        }

        source = err.source();
    }

    false
}
