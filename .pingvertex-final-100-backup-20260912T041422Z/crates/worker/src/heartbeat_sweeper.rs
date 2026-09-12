use std::{error::Error, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

use application::ports::MonitorRepository;
use chrono::{Duration as ChronoDuration, Utc};
use domain::{Monitor, MonitorKind};
use tokio::time;
use tracing::{error, info};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Passive heartbeat evaluation loop.
///
/// Heartbeat monitors are never actively checked on the claim loop: the
/// external system pings `GET /heartbeat/{token}`, which updates
/// `monitors.last_heartbeat_at`. This sweeper runs periodically, finds
/// every enabled Heartbeat monitor, and evaluates whether a heartbeat
/// arrived within the expected window (`heartbeat_interval_seconds` +
/// `heartbeat_grace_period_seconds`). It logs up/down status and records
/// Prometheus metrics, but it does NOT persist a `Check` — heartbeat
/// state is represented only by `last_heartbeat_at` itself.
#[derive(Clone)]
pub struct HeartbeatSweeper {
    monitor_repository: Arc<dyn MonitorRepository>,
}

impl HeartbeatSweeper {
    pub fn new(monitor_repository: Arc<dyn MonitorRepository>) -> Self {
        Self { monitor_repository }
    }

    /// Runs forever, sweeping once every `interval`.
    pub async fn run_forever(&self, interval: Duration, token: CancellationToken) -> Result<()> {
        let mut ticker = time::interval(interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("heartbeat sweeper cancellation requested; shutting down");
                    return Ok(());
                }
                _ = ticker.tick() => {
                    if let Err(err) = self.sweep_once().await {
                        error!(error = %err, "heartbeat sweep failed");
                    }
                }
            }
        }
    }

    /// Evaluates all enabled heartbeat monitors once.
    pub async fn sweep_once(&self) -> Result<()> {
        let monitors = self.monitor_repository.find_all().await?;

        for monitor in monitors {
            if monitor.kind != MonitorKind::Heartbeat || !monitor.enabled {
                continue;
            }

            match evaluate_heartbeat_status(&monitor) {
                HeartbeatStatus::Up { last_heartbeat_at } => {
                    info!(
                        monitor_id = %monitor.id.0,
                        monitor_name = %monitor.name.as_str(),
                        last_heartbeat_at = %last_heartbeat_at,
                        "heartbeat monitor is up"
                    );
                }
                HeartbeatStatus::Down { reason } => {
                    info!(
                        monitor_id = %monitor.id.0,
                        monitor_name = %monitor.name.as_str(),
                        reason = %reason,
                        "heartbeat monitor is down"
                    );
                }
            }
        }

        Ok(())
    }
}

/// Current pass/fail state of a heartbeat monitor.
#[derive(Debug, Clone)]
pub enum HeartbeatStatus {
    Up {
        last_heartbeat_at: chrono::DateTime<Utc>,
    },
    Down {
        reason: String,
    },
}

/// Pure evaluation of a heartbeat monitor's up/down status.
pub fn evaluate_heartbeat_status(monitor: &Monitor) -> HeartbeatStatus {
    let interval_seconds = match monitor.heartbeat_interval_seconds {
        Some(v) => v,
        None => {
            return HeartbeatStatus::Down {
                reason: "heartbeat_interval_seconds is missing".to_string(),
            };
        }
    };

    let grace_period_seconds = match monitor.heartbeat_grace_period_seconds {
        Some(v) => v,
        None => {
            return HeartbeatStatus::Down {
                reason: "heartbeat_grace_period_seconds is missing".to_string(),
            };
        }
    };

    match monitor.last_heartbeat_at {
        None => HeartbeatStatus::Down {
            reason: "no heartbeat received yet".to_string(),
        },
        Some(last_heartbeat_at) => {
            let deadline = last_heartbeat_at
                + ChronoDuration::seconds(
                    i64::from(interval_seconds) + i64::from(grace_period_seconds),
                );

            if Utc::now() > deadline {
                HeartbeatStatus::Down {
                    reason: format!(
                        "heartbeat overdue: last={} deadline={}",
                        last_heartbeat_at, deadline
                    ),
                }
            } else {
                HeartbeatStatus::Up { last_heartbeat_at }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::shared::ids::{MonitorId, TenantId, UserId};
    use domain::{
        CheckInterval, ExpectedStatus, HeartbeatToken, HttpMethod, MonitorBuilder, MonitorName,
        Timeout, Url,
    };
    use uuid::Uuid;

    fn build_heartbeat_monitor(
        interval: u32,
        grace: u32,
        last_heartbeat_at: Option<chrono::DateTime<Utc>>,
    ) -> Monitor {
        let mut monitor = MonitorBuilder::new(
            MonitorId(Uuid::new_v4()),
            UserId(Uuid::new_v4()),
            TenantId::new(),
        )
        .name(MonitorName::new("Heartbeat Test").unwrap())
        .url(Url::new("http://example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(60).unwrap())
        .timeout(Timeout::new(5).unwrap())
        .build();

        monitor.kind = MonitorKind::Heartbeat;
        monitor.heartbeat_token = Some(HeartbeatToken::generate());
        monitor.heartbeat_interval_seconds = Some(interval);
        monitor.heartbeat_grace_period_seconds = Some(grace);
        monitor.last_heartbeat_at = last_heartbeat_at;

        monitor
    }

    #[test]
    fn down_when_no_heartbeat_received() {
        let monitor = build_heartbeat_monitor(60, 30, None);
        assert!(matches!(
            evaluate_heartbeat_status(&monitor),
            HeartbeatStatus::Down { .. }
        ));
    }

    #[test]
    fn up_when_heartbeat_is_recent() {
        let monitor = build_heartbeat_monitor(60, 30, Some(Utc::now()));
        assert!(matches!(
            evaluate_heartbeat_status(&monitor),
            HeartbeatStatus::Up { .. }
        ));
    }

    #[test]
    fn down_when_heartbeat_is_overdue() {
        let old = Utc::now() - ChronoDuration::seconds(120);
        let monitor = build_heartbeat_monitor(60, 30, Some(old));
        assert!(matches!(
            evaluate_heartbeat_status(&monitor),
            HeartbeatStatus::Down { .. }
        ));
    }

    #[test]
    fn down_when_interval_field_missing() {
        let mut monitor = build_heartbeat_monitor(60, 30, Some(Utc::now()));
        monitor.heartbeat_interval_seconds = None;
        assert!(matches!(
            evaluate_heartbeat_status(&monitor),
            HeartbeatStatus::Down { .. }
        ));
    }
}
