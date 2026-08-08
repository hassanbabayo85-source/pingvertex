use std::{
    error::Error,
    time::{Duration, Instant},
};

use domain::{CheckResult, HttpMethod, Monitor};
use reqwest::Client;
use tracing::{info, warn};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Sends one HTTP check for a monitor and returns the result.
pub async fn check_monitor(client: &Client, monitor: &Monitor) -> Result<CheckResult> {
    if !monitor.enabled {
        return Ok(CheckResult::failure(0, "monitor is disabled"));
    }

    let method = match monitor.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Head => reqwest::Method::HEAD,
    };

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
                info!(
                    monitor_id = %monitor.id.0,
                    name = %monitor.name.as_str(),
                    status = status,
                    latency_ms = latency,
                    "monitor healthy"
                );

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
