use std::time::{Duration, Instant};

use application::MeterUsageInput;
use application::use_cases::ChargeForUsageError;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use domain::value_objects::HeartbeatToken;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::state::{AppState, HeartbeatRateLimiter};

/// Max number of `GET /public/heartbeat/{token}` requests a single token
/// may make within `HEARTBEAT_RATE_LIMIT_WINDOW`.
///
/// This is a PER-TOKEN limit, deliberately separate from the IP-based
/// `GovernorLayer` applied to the rest of `public::router()` — a heartbeat
/// token is typically pinged repeatedly from the SAME source IP (the
/// monitored service's own cron/health-check job), so an IP-based limit
/// alone would either be too loose (one IP hosting many monitored
/// services, e.g. a shared CI runner) or too tight (one IP legitimately
/// pinging many of its own heartbeat monitors). Keying on the token itself
/// gives each monitored service its own independent quota regardless of
/// source IP.
const HEARTBEAT_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const HEARTBEAT_RATE_LIMIT_MAX_REQUESTS: u32 = 6;
const MAX_HEARTBEAT_IDEMPOTENCY_KEY_BYTES: usize = 128;

/// `GET /api/v1/public/heartbeat/{token}`
///
/// Unauthenticated "dead man's switch" ping endpoint for
/// `MonitorKind::Heartbeat` monitors. A monitored service (cron job,
/// background worker, etc.) calls this on its own schedule; the worker's
/// scheduler separately evaluates whether a heartbeat has been received
/// recently enough (see `evaluate_heartbeat_monitor` in
/// `worker::scheduler`) and opens/resolves an incident accordingly.
///
/// SECURITY: never returns any monitor details (name, URL, owner, etc.) —
/// only a generic acknowledgement — since this endpoint is unauthenticated
/// and reachable by anyone who has (or guesses/leaks) a token.
///
/// B34: the raw path segment is validated as a well-formed
/// `HeartbeatToken` BEFORE it is ever used as a key into the in-memory
/// rate-limit map. Previously, any arbitrary string reaching this handler
/// — including garbage that could never match a real monitor — was
/// inserted into `HeartbeatRateLimiter` first, and only rejected by the
/// DB lookup afterwards. Since that map is never pruned (see its doc
/// comment) and is keyed by the raw path string, an attacker could grow
/// it without bound by requesting a large number of distinct nonsense
/// paths — a memory-exhaustion vector on an unauthenticated endpoint,
/// independent of (and not mitigated by) the IP-based `GovernorLayer` on
/// `public::router()`, since a distributed/rotating-IP attacker is not
/// slowed by a per-IP limit. Rejecting malformed tokens before the map
/// insert closes this: only strings shaped like a real token (UUID or
/// 32-char lowercase hex — see `HeartbeatToken::new`) can ever occupy a
/// slot, and the number of distinct VALID tokens is bounded by how many
/// heartbeat monitors actually exist.
pub async fn record_heartbeat(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let token = HeartbeatToken::new(&token).map_err(|_| StatusCode::NOT_FOUND)?;

    if !check_and_record_rate_limit(&state.heartbeat_rate_limiter, token.as_str()).await {
        warn!("heartbeat rate limit exceeded for a token");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let monitor = match state
        .monitor_repository
        .find_by_heartbeat_token(token.as_str())
        .await
    {
        Ok(Some(monitor)) => monitor,
        Ok(None) => return Err(StatusCode::NOT_FOUND),
        Err(err) => {
            warn!(error = %err, "failed to look up monitor by heartbeat token");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // BILLING POLICY: a heartbeat is a user-triggered passive monitoring
    // operation, so PAYG meters the accepted heartbeat receipt itself. The
    // sweeper's later timeout evaluation is platform-side bookkeeping and
    // is intentionally NOT billed.
    let now = Utc::now();
    let request_id = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= MAX_HEARTBEAT_IDEMPOTENCY_KEY_BYTES
                && value.is_ascii()
                && value.bytes().all(|b| b.is_ascii_graphic())
        })
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!(
                "heartbeat:{}:{}",
                monitor.id.0,
                now.timestamp_nanos_opt().unwrap_or_default()
            )
        });

    if let Some(metering) = &state.payg_metering {
        match metering
            .execute(
                MeterUsageInput {
                    tenant_id: monitor.tenant_id.0,
                    operation: "heartbeat_ping".to_owned(),
                    units: 1,
                    request_id: format!("heartbeat:{}", request_id),
                },
                || {
                    state
                        .monitor_repository
                        .update_last_heartbeat_at(monitor.id, now)
                },
            )
            .await
        {
            Ok(_) => {}
            Err(ChargeForUsageError::InsufficientBalance) => {
                warn!(monitor_id = %monitor.id.0, "heartbeat rejected: insufficient PAYG balance");
                return Err(StatusCode::PAYMENT_REQUIRED);
            }
            Err(err) => {
                warn!(monitor_id = %monitor.id.0, error = %err, "heartbeat metering/record failed");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else if let Err(err) = state
        .monitor_repository
        .update_last_heartbeat_at(monitor.id, now)
        .await
    {
        warn!(monitor_id = %monitor.id.0, error = %err, "failed to record heartbeat");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    info!(monitor_id = %monitor.id.0, "heartbeat recorded");

    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

/// Fixed-window per-token rate limiter. Returns `true` if this request is
/// allowed (and records it against the token's current window), `false` if
/// the token has already made `HEARTBEAT_RATE_LIMIT_MAX_REQUESTS` requests
/// within the current `HEARTBEAT_RATE_LIMIT_WINDOW`.
///
/// NOTE: the map is never pruned of stale/inactive tokens. This is bounded
/// in practice by the number of Heartbeat monitors that have EVER pinged
/// this server (one entry per distinct token) AND, as of B34, only ever
/// receives entries for well-formed tokens (see `record_heartbeat`'s doc
/// comment) — a small, slowly-growing set compared to raw request volume,
/// acceptable for now, but a candidate for a periodic sweep if it ever
/// becomes a concern.
async fn check_and_record_rate_limit(limiter: &HeartbeatRateLimiter, token: &str) -> bool {
    let mut buckets = limiter.lock().await;
    let now = Instant::now();

    match buckets.get_mut(token) {
        Some((window_started_at, count)) => {
            if now.duration_since(*window_started_at) >= HEARTBEAT_RATE_LIMIT_WINDOW {
                *window_started_at = now;
                *count = 1;
                true
            } else if *count < HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
                *count += 1;
                true
            } else {
                false
            }
        }
        None => {
            buckets.insert(token.to_string(), (now, 1));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn empty_limiter() -> HeartbeatRateLimiter {
        Arc::new(tokio::sync::Mutex::new(HashMap::new()))
    }

    #[tokio::test]
    async fn allows_requests_up_to_the_limit() {
        let limiter = empty_limiter();

        for _ in 0..HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
            assert!(check_and_record_rate_limit(&limiter, "token-a").await);
        }
    }

    #[tokio::test]
    async fn rejects_requests_beyond_the_limit_within_the_window() {
        let limiter = empty_limiter();

        for _ in 0..HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
            assert!(check_and_record_rate_limit(&limiter, "token-b").await);
        }

        assert!(!check_and_record_rate_limit(&limiter, "token-b").await);
    }

    #[tokio::test]
    async fn different_tokens_have_independent_limits() {
        let limiter = empty_limiter();

        for _ in 0..HEARTBEAT_RATE_LIMIT_MAX_REQUESTS {
            assert!(check_and_record_rate_limit(&limiter, "token-c").await);
        }

        assert!(check_and_record_rate_limit(&limiter, "token-d").await);
    }

    #[tokio::test]
    async fn record_heartbeat_rejects_malformed_token_before_touching_rate_limiter() {
        // B34 regression test: a malformed token must never be able to
        // grow the rate-limiter map. We can't easily call the full
        // handler here without a lot of AppState scaffolding (already
        // covered end-to-end in crates/api/tests/heartbeat.rs), so this
        // test asserts the underlying guarantee directly: HeartbeatToken
        // parsing rejects arbitrary garbage, which is the mechanism
        // `record_heartbeat` now relies on to fail closed before any
        // limiter/database call.
        assert!(HeartbeatToken::new("../../../../etc/passwd").is_err());
        assert!(HeartbeatToken::new("<script>alert(1)</script>").is_err());
        assert!(HeartbeatToken::new("").is_err());
    }
}
