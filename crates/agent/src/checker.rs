use domain::{HttpMethod, Monitor, RegionCode};
use reqwest::Client;
use serde::Serialize;
use shared::ssrf::resolve_and_validate_destination;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct CheckOutcome {
    pub success: bool,
    pub status_code: Option<u16>,
    pub latency_ms: u128,
    pub error: Option<String>,
    pub region: String,
}

/// Executes a single HTTP check for a monitor from the given region.
///
/// SSRF protection uses `shared::ssrf::resolve_and_validate_destination`
/// — the SAME validator worker uses — before every request. The returned
/// `CheckOutcome` is serialized and submitted to the central API.
pub async fn check_monitor(monitor: &Monitor, region: &RegionCode) -> CheckOutcome {
    if !monitor.enabled {
        return CheckOutcome {
            success: false,
            status_code: None,
            latency_ms: 0,
            error: Some("monitor is disabled".to_string()),
            region: region.to_string(),
        };
    }

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

    let overall_started = Instant::now();

    let destination = match resolve_and_validate_destination(monitor.url.as_str()).await {
        Ok(destination) => destination,
        Err(err) => {
            let latency = overall_started.elapsed().as_millis();
            return CheckOutcome {
                success: false,
                status_code: None,
                latency_ms: latency,
                error: Some(err),
                region: region.to_string(),
            };
        }
    };

    let client = match Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            let latency = overall_started.elapsed().as_millis();
            return CheckOutcome {
                success: false,
                status_code: None,
                latency_ms: latency,
                error: Some(err.to_string()),
                region: region.to_string(),
            };
        }
    };

    let started = Instant::now();
    let response = client
        .request(method, destination.url.clone())
        .timeout(Duration::from_secs(monitor.timeout_seconds.into()))
        .send()
        .await;

    let latency = started.elapsed().as_millis();

    match response {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let success = status == monitor.expected_status;
            CheckOutcome {
                success,
                status_code: Some(status),
                latency_ms: latency,
                error: if success {
                    None
                } else {
                    Some(format!(
                        "expected {}, got {}",
                        monitor.expected_status, status
                    ))
                },
                region: region.to_string(),
            }
        }
        Err(err) => CheckOutcome {
            success: false,
            status_code: None,
            latency_ms: latency,
            error: Some(err.to_string()),
            region: region.to_string(),
        },
    }
}
