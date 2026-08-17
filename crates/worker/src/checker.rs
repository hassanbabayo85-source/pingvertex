use std::{
    collections::HashMap,
    error::Error,
    net::SocketAddr,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use domain::{CheckResult, HttpMethod, Monitor, RegionCode};
use reqwest::Client;
use tracing::{info, warn};

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};
use crate::diagnostics;

type Result = std::result::Result<CheckResult, Box<dyn Error + Send + Sync>>;

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

/// Send a single HTTP request for a monitor and return the check result.
pub async fn check_monitor(_client: &Client, monitor: &Monitor) -> Result {
    if !monitor.enabled {
        return Ok(CheckResult::failure(
            RegionCode::default_region(),
            0,
            "monitor is disabled",
        ));
    }

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let overall_started = Instant::now();

    // Resolve DNS + validate IP exactly once.
    //
    // IMPORTANT: this is intentionally NOT propagated via `?` anymore. A
    // DNS/SSRF failure here IS a real "Down" result for this monitor
    // (ErrorType::Dns territory) and must be persisted as a Check like
    // any other failure — including a diagnostic report explaining that
    // DNS resolution failed. Silently short-circuiting via `?` would skip
    // saving a Check entirely (see `scheduler.rs`'s `Ok((monitor, Err(_)))`
    // branch, which only logs, it never calls `check_repository.save`).
    let destination =
        match crate::ssrf::resolve_and_validate_destination(monitor.url.as_str()).await {
            Ok(destination) => destination,
            Err(err) => {
                let latency = overall_started.elapsed().as_millis();
                let diag = diagnostics::run_diagnostics(monitor, None).await;

                warn!(
                    monitor_id = %monitor.id.0,
                    name = %monitor.name.as_str(),
                    error = %err,
                    "failed to resolve/validate monitor destination"
                );

                return Ok(
                    CheckResult::failure(RegionCode::default_region(), latency, err)
                        .with_diagnostics(diag.steps, diag.root_cause_summary),
                );
            }
        };

    // Keep the original hostname in the URL so HTTPS certificate validation
    // and SNI continue to use the real hostname.
    let pinned_client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            let latency = overall_started.elapsed().as_millis();
            let diag = diagnostics::run_diagnostics(monitor, Some(&destination)).await;

            return Ok(CheckResult::failure(
                RegionCode::default_region(),
                latency,
                err.to_string(),
            )
            .with_diagnostics(diag.steps, diag.root_cause_summary));
        }
    };

    let started = Instant::now();

    let response = pinned_client
        .request(method, destination.url.clone())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    let latency = started.elapsed().as_millis();

    match response {
        Ok(response) => {
            let status = response.status().as_u16();

            if status == monitor.expected_status {
                // Evaluate assertions if present
                if !monitor.assertions.is_empty() {
                    let headers: HashMap<String, String> = response
                        .headers()
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.as_str().to_lowercase(),
                                v.to_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect();

                    let body_bytes = response.bytes().await.unwrap_or_default().to_vec();

                    let ctx = EvalContext {
                        status_code: status,
                        headers,
                        body_bytes,
                        latency_ms: latency,
                    };

                    let (all_passed, results) = evaluate_assertions(&monitor.assertions, &ctx);

                    if !all_passed {
                        let errors: Vec<String> =
                            results.iter().filter_map(|r| r.message.clone()).collect();

                        let error_msg = format!("assertions sun gaza: {}", errors.join("; "));

                        warn!(
                            monitor_id = %monitor.id.0,
                            name = %monitor.name.as_str(),
                            latency_ms = latency,
                            "assertion failure: {}",
                            error_msg
                        );

                        // Assertion failures are NOT connectivity problems —
                        // the server responded correctly at the HTTP level,
                        // so no diagnostic run (DNS/TCP/TLS/HTTP) is needed
                        // here; the assertion message already explains why.
                        return Ok(CheckResult::failure_with_status(
                            RegionCode::default_region(),
                            status,
                            latency,
                            error_msg,
                        ));
                    }

                    info!(
                        monitor_id = %monitor.id.0,
                        name = %monitor.name.as_str(),
                        status = status,
                        latency_ms = latency,
                        "monitor healthy (all assertions passed)"
                    );
                } else {
                    info!(
                        monitor_id = %monitor.id.0,
                        name = %monitor.name.as_str(),
                        status = status,
                        latency_ms = latency,
                        "monitor healthy"
                    );
                }

                let (cert_expires_at, cert_days_remaining) =
                    maybe_fetch_cert_info(monitor, &destination.host, destination.socket_addr)
                        .await;

                Ok(
                    CheckResult::success(RegionCode::default_region(), status, latency)
                        .with_cert_info(cert_expires_at, cert_days_remaining),
                )
            } else {
                warn!(
                    monitor_id = %monitor.id.0,
                    name = %monitor.name.as_str(),
                    expected = monitor.expected_status,
                    actual = status,
                    latency_ms = latency,
                    "unexpected status code"
                );

                Ok(CheckResult::failure(
                    RegionCode::default_region(),
                    latency,
                    format!("expected {}, got {}", monitor.expected_status, status),
                ))
            }
        }

        Err(err) => {
            warn!(
                monitor_id = %monitor.id.0,
                name = %monitor.name.as_str(),
                latency_ms = latency,
                error = %err,
                "monitor request failed"
            );

            // DNS/SSRF already succeeded (we have `destination`) but the
            // actual HTTP request failed — this is TCP/TLS/HTTP territory,
            // so run diagnostics reusing the already-validated destination
            // (no second DNS lookup).
            let diag = diagnostics::run_diagnostics(monitor, Some(&destination)).await;

            Ok(
                CheckResult::failure(RegionCode::default_region(), latency, err.to_string())
                    .with_diagnostics(diag.steps, diag.root_cause_summary),
            )
        }
    }
}

/// Send a single HTTP request for a monitor using an injected destination
/// validator.
///
/// Production code should normally call `check_monitor`, which uses the real
/// SSRF validator. Tests can inject a validator so they can use a local HTTP
/// server without weakening the production SSRF protection.
///
/// NOTE: this test-only path does not run diagnostics — it has no
/// `ResolvedDestination` to work with (the injected validator only
/// validates, it doesn't resolve/pin), so it is out of scope for the
/// diagnostics feature. Production traffic always goes through
/// `check_monitor` above.
pub async fn check_monitor_with_validator<F, Fut>(
    client: &Client,
    monitor: &Monitor,
    validate_destination: F,
) -> Result
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), Box<dyn Error + Send + Sync>>>,
{
    if !monitor.enabled {
        return Ok(CheckResult::failure(
            RegionCode::default_region(),
            0,
            "monitor is disabled",
        ));
    }

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    validate_destination(monitor.url.as_str().to_string()).await?;

    let started = Instant::now();

    let response = client
        .request(method, monitor.url.as_str())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    let latency = started.elapsed().as_millis();

    match response {
        Ok(response) => {
            let status = response.status().as_u16();

            if status == monitor.expected_status {
                if !monitor.assertions.is_empty() {
                    let headers: HashMap<String, String> = response
                        .headers()
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.as_str().to_lowercase(),
                                v.to_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect();

                    let body_bytes = response.bytes().await.unwrap_or_default().to_vec();

                    let ctx = EvalContext {
                        status_code: status,
                        headers,
                        body_bytes,
                        latency_ms: latency,
                    };

                    let (all_passed, results) = evaluate_assertions(&monitor.assertions, &ctx);

                    if !all_passed {
                        let errors: Vec<String> =
                            results.iter().filter_map(|r| r.message.clone()).collect();

                        let error_msg = format!("assertions sun gaza: {}", errors.join("; "));

                        warn!(
                            monitor_id = %monitor.id.0,
                            name = %monitor.name.as_str(),
                            latency_ms = latency,
                            "assertion failure: {}",
                            error_msg
                        );

                        return Ok(CheckResult::failure_with_status(
                            RegionCode::default_region(),
                            status,
                            latency,
                            error_msg,
                        ));
                    }

                    info!(
                        monitor_id = %monitor.id.0,
                        name = %monitor.name.as_str(),
                        status = status,
                        latency_ms = latency,
                        "monitor healthy (all assertions passed)"
                    );
                } else {
                    info!(
                        monitor_id = %monitor.id.0,
                        name = %monitor.name.as_str(),
                        status = status,
                        latency_ms = latency,
                        "monitor healthy"
                    );
                }

                Ok(CheckResult::success(
                    RegionCode::default_region(),
                    status,
                    latency,
                ))
            } else {
                warn!(
                    monitor_id = %monitor.id.0,
                    name = %monitor.name.as_str(),
                    expected = monitor.expected_status,
                    actual = status,
                    latency_ms = latency,
                    "unexpected status code"
                );

                Ok(CheckResult::failure(
                    RegionCode::default_region(),
                    latency,
                    format!("expected {}, got {}", monitor.expected_status, status),
                ))
            }
        }

        Err(err) => {
            warn!(
                monitor_id = %monitor.id.0,
                name = %monitor.name.as_str(),
                latency_ms = latency,
                error = %err,
                "monitor request failed"
            );

            Ok(CheckResult::failure(
                RegionCode::default_region(),
                latency,
                err.to_string(),
            ))
        }
    }
}
