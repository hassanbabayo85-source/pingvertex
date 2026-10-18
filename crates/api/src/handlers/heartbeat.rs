use application::MeterUsageInput;
use application::use_cases::ChargeForUsageError;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
};
use chrono::Utc;
use domain::value_objects::HeartbeatToken;
use serde_json::{Value, json};
use tracing::{info, warn};

use crate::state::AppState;

/// Max number of heartbeat requests a single token may make within
/// `HEARTBEAT_RATE_LIMIT_WINDOW_SECS`.
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
///
/// This is enforced via `state.rate_limiter` — the same
/// `RateLimiter` port the RUM ingest endpoint uses — which in production
/// is `infra::RedisRateLimiter` with a real per-key TTL. This replaces
/// the previous per-process, never-pruned `HashMap` bucket which (a)
/// grew without bound as new tokens were seen and (b) was per-instance,
/// so a token could exceed its nominal limit N times over when the
/// service ran in N replicas behind a load balancer.
const HEARTBEAT_RATE_LIMIT_WINDOW_SECS: u64 = 60;
const HEARTBEAT_RATE_LIMIT_MAX_REQUESTS: u32 = 6;

/// Namespace used with `state.rate_limiter`, kept stable so counters do
/// not collide with the RUM ingest or auth namespaces.
const HEARTBEAT_RATE_LIMIT_NAMESPACE: &str = "heartbeat";

const MAX_HEARTBEAT_IDEMPOTENCY_KEY_BYTES: usize = 128;

/// Bucket size (seconds) applied to a client-supplied `Idempotency-Key`.
///
/// An `Idempotency-Key` is only treated as identifying the
/// SAME request within a single bucket of this size. Past the bucket
/// boundary, the same key value identifies a NEW request — so a client
/// that caches or reuses an idempotency key cannot silently suppress
/// legitimate future heartbeat execution.
///
/// Chosen (a) larger than `HEARTBEAT_RATE_LIMIT_WINDOW_SECS` so an
/// in-flight retry inside one rate-limit burst still dedupes, and (b)
/// smaller than any realistic heartbeat interval so a reused key never
/// suppresses more than one scheduled ping.
const HEARTBEAT_IDEMPOTENCY_BUCKET_SECS: i64 = 300;

/// `GET /api/v1/public/heartbeat/{token}` (LEGACY).
///
/// Kept for backwards compatibility with clients that already embed the
/// token in a URL. New integrations should use
/// `POST /api/v1/public/heartbeat` with the `X-Heartbeat-Token` header
/// (or `Authorization: Bearer <token>`) — see [`record_heartbeat_post`]
/// — so the credential never appears in a URL path, access log, browser
/// history, or `Referer` header.
///
/// `routes::public::router()` sets `Referrer-Policy: no-referrer` on
/// every response from this route group, so a browser that somehow loads
/// this URL cannot leak it via `Referer` on the next navigation.
pub async fn record_heartbeat(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    record_heartbeat_inner(&state, &token, &headers).await
}

/// `POST /api/v1/public/heartbeat` (RECOMMENDED).
///
/// Token is supplied in a header, never in the URL:
///   * `X-Heartbeat-Token: <token>` (preferred), or
///   * `Authorization: Bearer <token>`.
///
/// If both are present, `X-Heartbeat-Token` wins (more specific for this
/// endpoint). Response body is identical to the GET shape — no monitor
/// details are ever returned by either.
///
/// SECURITY: the token value is never logged by this handler and is
/// never included in the response body — see requirement "Never expose
/// monitor details from heartbeat endpoint" / "Do not log the heartbeat
/// token".
pub async fn record_heartbeat_post(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let token = extract_heartbeat_token_from_headers(&headers).ok_or(StatusCode::NOT_FOUND)?;
    record_heartbeat_inner(&state, &token, &headers).await
}

/// Extracts a heartbeat token from the request headers.
///
/// Returns `None` when neither header is present or when neither parses
/// as text; the caller treats that exactly like an invalid token. Full
/// shape validation still happens in [`HeartbeatToken::new`].
///
/// SECURITY: never logged, never echoed back.
fn extract_heartbeat_token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-heartbeat-token")
        && let Ok(s) = value.to_str()
    {
        return Some(s.to_string());
    }
    if let Some(value) = headers.get(header::AUTHORIZATION)
        && let Ok(s) = value.to_str()
    {
        if let Some(rest) = s.strip_prefix("Bearer ") {
            return Some(rest.to_string());
        }
        if let Some(rest) = s.strip_prefix("bearer ") {
            return Some(rest.to_string());
        }
    }
    None
}

/// Shared handler body for both GET and POST shapes.
async fn record_heartbeat_inner(
    state: &AppState,
    raw_token: &str,
    headers: &HeaderMap,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    // (1) Reject malformed tokens BEFORE any rate-limit or DB
    // work. Only well-formed `HeartbeatToken`s ever reach the limiter, so
    // an attacker cannot grow the limiter's key space with junk.
    let token = HeartbeatToken::new(raw_token).map_err(|_| StatusCode::NOT_FOUND)?;

    // (2) Distributed, TTL-bounded per-token rate limit.
    // Fail CLOSED on limiter unavailability, matching rum_ingest.
    let allowed = match state
        .rate_limiter
        .check_and_record(
            HEARTBEAT_RATE_LIMIT_NAMESPACE,
            token.as_str(),
            HEARTBEAT_RATE_LIMIT_MAX_REQUESTS,
            HEARTBEAT_RATE_LIMIT_WINDOW_SECS,
        )
        .await
    {
        Ok(allowed) => allowed,
        Err(err) => {
            warn!(error = %err, "heartbeat rate limiter unavailable; failing closed");
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
    };
    if !allowed {
        // No token in the log line.
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
    let request_id = build_request_id(&monitor.id, headers, now);

    if let Some(metering) = &state.payg_metering {
        match metering
            .execute(
                MeterUsageInput {
                    tenant_id: monitor.tenant_id.0,
                    operation: domain::PaygOperation::HeartbeatProcessing
                        .as_str()
                        .to_owned(),
                    units: 1,
                    request_id,
                    monitor_id: None,
                    execution_id: None,
                    pricing_context: None,
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

/// Builds the idempotency `request_id` for one heartbeat attempt.
///
/// * No client `Idempotency-Key` — a unique value per request
///   (`heartbeat:<monitor_id>:<nanos>`), so retries of the SAME real
///   request still charge independently. This is the default.
/// * With client `Idempotency-Key` — `heartbeat:<monitor_id>:<key>:<bucket>`.
///   Within one bucket the same key is deduplicated; in a new bucket the
///   same key identifies a distinct request. This bounds how long a
///   stale/reused key can suppress a legitimate future heartbeat
///   invariant.
///
/// The token is NEVER part of the request_id (logging invariant).
fn build_request_id(
    monitor_id: &domain::shared::ids::MonitorId,
    headers: &HeaderMap,
    now: chrono::DateTime<Utc>,
) -> String {
    let client_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= MAX_HEARTBEAT_IDEMPOTENCY_KEY_BYTES
                && value.is_ascii()
                && value.bytes().all(|b| b.is_ascii_graphic())
        });

    match client_key {
        Some(key) => {
            let bucket = now.timestamp() / HEARTBEAT_IDEMPOTENCY_BUCKET_SECS;
            format!("heartbeat:{}:{}:{}", monitor_id.0, key, bucket)
        }
        None => format!(
            "heartbeat:{}:{}",
            monitor_id.0,
            now.timestamp_nanos_opt().unwrap_or_default()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn extracts_token_from_x_heartbeat_token_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-heartbeat-token",
            HeaderValue::from_static("550e8400-e29b-41d4-a716-446655440000"),
        );
        assert_eq!(
            extract_heartbeat_token_from_headers(&headers).as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
    }

    #[test]
    fn extracts_token_from_authorization_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer 550e8400-e29b-41d4-a716-446655440000"),
        );
        assert_eq!(
            extract_heartbeat_token_from_headers(&headers).as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
    }

    #[test]
    fn x_heartbeat_token_wins_over_authorization_when_both_present() {
        let mut headers = HeaderMap::new();
        headers.insert("x-heartbeat-token", HeaderValue::from_static("winner"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer loser"),
        );
        assert_eq!(
            extract_heartbeat_token_from_headers(&headers).as_deref(),
            Some("winner")
        );
    }

    #[test]
    fn no_token_headers_yields_none() {
        let headers = HeaderMap::new();
        assert!(extract_heartbeat_token_from_headers(&headers).is_none());
    }

    #[test]
    fn malformed_authorization_scheme_yields_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );
        assert!(extract_heartbeat_token_from_headers(&headers).is_none());
    }

    #[test]
    fn build_request_id_is_unique_without_client_key() {
        let monitor_id = domain::shared::ids::MonitorId(uuid::Uuid::new_v4());
        let headers = HeaderMap::new();
        let now = Utc::now();
        let a = build_request_id(&monitor_id, &headers, now);
        let b = build_request_id(
            &monitor_id,
            &headers,
            now + chrono::Duration::nanoseconds(1),
        );
        assert_ne!(a, b);
        assert!(!a.contains("idempotency"));
    }

    #[test]
    fn build_request_id_buckets_client_key_by_time() {
        let monitor_id = domain::shared::ids::MonitorId(uuid::Uuid::new_v4());
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", HeaderValue::from_static("abc"));
        let t1 = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let a = build_request_id(&monitor_id, &headers, t1);
        let b = build_request_id(&monitor_id, &headers, t1 + chrono::Duration::seconds(10));
        assert_eq!(a, b);
        let c = build_request_id(
            &monitor_id,
            &headers,
            t1 + chrono::Duration::seconds(HEARTBEAT_IDEMPOTENCY_BUCKET_SECS),
        );
        assert_ne!(a, c);
    }

    #[test]
    fn build_request_id_never_contains_the_token() {
        let monitor_id = domain::shared::ids::MonitorId(uuid::Uuid::new_v4());
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", HeaderValue::from_static("abc"));
        headers.insert(
            "x-heartbeat-token",
            HeaderValue::from_static("550e8400-e29b-41d4-a716-446655440000"),
        );
        let id = build_request_id(&monitor_id, &headers, Utc::now());
        assert!(!id.contains("550e8400"));
    }

    #[test]
    fn malformed_tokens_are_rejected_by_the_value_object() {
        assert!(HeartbeatToken::new("../../../../etc/passwd").is_err());
        assert!(HeartbeatToken::new("<script>alert(1)</script>").is_err());
        assert!(HeartbeatToken::new("").is_err());
    }
}
