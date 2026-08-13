use std::{
    collections::HashMap,
    error::Error,
    net::SocketAddr,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use domain::{CheckResult, HttpMethod, Monitor};
use reqwest::Client;
use tracing::{info, warn};

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};

type Result = std::result::Result<CheckResult, Box<dyn Error + Send + Sync>>;

/// Fetches TLS certificate expiry info for `monitor`'s endpoint, if
/// `monitor.ssl_check_enabled` is set and `monitor.url` is `https://`.
/// Otherwise returns `(None, None)` immediately without any network I/O.
///
/// `hostname` and `socket_addr` MUST already have been through SSRF
/// validation by the caller — this mirrors the exact safety contract
/// documented on `tls_cert::fetch_certificate_info` itself. In
/// `check_monitor` below, both are taken from the `ResolvedDestination`
/// that `crate::ssrf::resolve_and_validate_destination` already produced
/// for the HTTP request, so no second DNS resolution happens here.
///
/// This never fails the overall check: a certificate fetch error (bad
/// handshake, timeout, untrusted/expired cert, ...) is logged as a
/// `warn!` and simply results in `(None, None)` — the monitor's HTTP
/// check already succeeded by the time this is called, and a cert-fetch
/// problem alongside a successful HTTP response is informational, not a
/// reason to mark the monitor down.
///
/// Exposed as `pub` (rather than private) so integration tests in
/// `crates/worker/tests/checker.rs` can exercise this wiring directly
/// against a local test TLS server, the same way `check_monitor_with_validator`
/// lets tests bypass the production SSRF resolver without weakening it.
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
        return Ok(CheckResult::failure(0, "monitor is disabled"));
    }

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    // Resolve DNS + validate IP exactly once.
    //
    // The returned socket address is then pinned into the reqwest client
    // below. This prevents a second DNS lookup from reaching a different
    // address after validation.
    let destination = crate::ssrf::resolve_and_validate_destination(monitor.url.as_str())
        .await
        .map_err(Box::<dyn Error + Send + Sync>::from)?;

    // Keep the original hostname in the URL so HTTPS certificate validation
    // and SNI continue to use the real hostname.
    //
    // `resolve()` tells reqwest to use the already validated socket address
    // instead of performing another DNS lookup for this hostname.
    //
    // Redirects remain disabled because a redirect to an internal address
    // would otherwise create another SSRF path.
    let pinned_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .build()
        .map_err(Box::<dyn Error + Send + Sync>::from)?;

    let started = Instant::now();

    let response = pinned_client
        .request(method, destination.url)
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

                        return Ok(CheckResult::failure_with_status(status, latency, error_msg));
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

                // Reuse the SSRF-validated `destination.host`/`socket_addr`
                // from above — no second DNS resolution happens here. This
                // is a no-op ((None, None), no I/O) for monitors that don't
                // have ssl_check_enabled or aren't https://.
                let (cert_expires_at, cert_days_remaining) =
                    maybe_fetch_cert_info(monitor, &destination.host, destination.socket_addr)
                        .await;

                Ok(CheckResult::success(status, latency)
                    .with_cert_info(cert_expires_at, cert_days_remaining))
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

            Ok(CheckResult::failure(latency, err.to_string()))
        }
    }
}

/// Send a single HTTP request for a monitor using an injected destination
/// validator.
///
/// Production code should normally call `check_monitor`, which uses the real
/// SSRF validator. Tests can inject a validator so they can use a local HTTP
/// server without weakening the production SSRF protection.
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
        return Ok(CheckResult::failure(0, "monitor is disabled"));
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

                        return Ok(CheckResult::failure_with_status(status, latency, error_msg));
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

                Ok(CheckResult::success(status, latency))
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

            Ok(CheckResult::failure(latency, err.to_string()))
        }
    }
}
