use std::net::SocketAddr;

use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use tracing::warn;

use application::dto::RumIngestRequest;
use application::use_cases::{IngestRumEventError, IngestRumEventUseCase};

use crate::state::AppState;

/// Max accepted beacon body size, enforced HERE in addition to the global
/// `DefaultBodyLimit` in `router.rs`.
///
/// This endpoint deliberately does NOT rely on axum's `Json<T>` extractor
/// (see `collect_rum_event`'s doc comment for why), so it also doesn't get
/// that extractor's implicit content-length handling for free — this
/// constant is the equivalent backstop for this one route.
const MAX_RUM_BEACON_BYTES: usize = 8 * 1024;

/// Per-site fixed-window limit for `POST /api/v1/public/rum/collect`,
/// keyed by `site_key`.
///
/// Deliberately generous compared to per-account, per-user limits
/// elsewhere in this API (e.g. `FORGOT_PASSWORD_MAX_REQUESTS`): a single
/// popular site can legitimately generate many beacons per second from
/// many different real visitors. This is a backstop against abuse (e.g.
/// someone scripting thousands of fake beacons at one site_key), not a
/// per-visitor throttle — there is no reliable per-visitor identity here
/// by design (see Task D: no PII, no cookies).
///
/// This is IN ADDITION TO the IP-based `GovernorLayer` already applied to
/// the whole `public::router()` group in `router.rs`, mirroring the
/// "per-token limiter + per-IP governor" combination used by
/// `handlers::heartbeat`.
const RUM_INGEST_MAX_REQUESTS_PER_SITE: u32 = 200;
const RUM_INGEST_WINDOW_SECS: u64 = 10;

/// Checks that a string is shaped like a `RumSite::generate_site_key()`
/// output (`rum_` + 32 lowercase hex chars) BEFORE it's used as a rate
/// limiter key or a database query parameter.
///
/// This mirrors `HeartbeatToken::new`'s validate-before-use pattern (see
/// `handlers::heartbeat`'s doc comment on why that ordering matters): it's
/// a cheap, early rejection of obviously-malformed input, not the primary
/// security control (site existence is still authoritatively checked by
/// `RumSiteRepository::find_by_site_key`, and the rate limiter's Redis
/// keys self-expire regardless — see `RedisRateLimiter`).
fn is_well_formed_site_key(site_key: &str) -> bool {
    match site_key.strip_prefix("rum_") {
        Some(rest) => rest.len() == 32 && rest.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// `POST /api/v1/public/rum/collect`
///
/// Unauthenticated ingestion endpoint for the RUM browser SDK (Task D).
///
/// Deliberately does NOT use axum's `Json<T>` extractor, and deliberately
/// has NO CORS-specific handling. Both follow from the same transport
/// choice: the SDK sends beacons via `navigator.sendBeacon` (or a
/// `fetch(..., {mode: "no-cors"})` fallback) with the body's `Content-
/// Type` set to `text/plain` — NOT `application/json` — specifically so
/// the browser treats it as a CORS "simple request" and never issues a
/// preflight or requires this server to grant the sending origin CORS
/// access. `Json<T>` validates the `Content-Type` header and would reject
/// a `text/plain` body outright — hence the manual `Bytes` +
/// `serde_json::from_slice` below.
///
/// GEO-IP (Task E): `ConnectInfo<SocketAddr>` gives this handler the
/// TCP-level peer address of the request, which is used ONLY to resolve
/// a country code via `state.geo_ip_lookup` before the beacon is handed
/// to `IngestRumEventUseCase` — the browser SDK itself never collects or
/// sends any geography (see Task D). A geo-IP miss (unknown IP, lookup
/// unconfigured, decode failure) degrades to `country_code = None`; it
/// never fails ingestion. NOTE: this reads the raw TCP peer address, so
/// if this service ever sits behind a reverse proxy/load balancer without
/// PROXY protocol or an equivalent trusted-forwarded-for setup, this will
/// resolve to the proxy's IP rather than the visitor's — see
/// `IngestRumEventUseCase::execute`'s doc comment for the same caveat.
/// `ConnectInfo<SocketAddr>` requires the server to be started with
/// `.into_make_service_with_connect_info::<SocketAddr>()` in `main.rs`;
/// without that, this extractor fails every request.
///
/// SECURITY:
/// - Tenant/site resolution happens ONLY via `RumSiteRepository::
///   find_by_site_key` inside `IngestRumEventUseCase` — nothing in the
///   request body is trusted as a tenant or site identifier.
/// - Malformed input (including non-JSON bodies) is rejected with a
///   generic 400/404, never a panic.
/// - Errors are logged server-side only; the response body never echoes
///   back raw request content or internal error detail.
pub async fn collect_rum_event(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    if body.len() > MAX_RUM_BEACON_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({ "status": "rejected" })),
        );
    }

    let payload = match serde_json::from_slice::<RumIngestRequest>(&body) {
        Ok(payload) => payload,
        Err(err) => {
            warn!(error = %err, "RUM beacon rejected: not valid JSON for the expected shape");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "rejected" })),
            );
        }
    };

    if !is_well_formed_site_key(&payload.site_key) {
        return (StatusCode::NOT_FOUND, Json(json!({ "status": "rejected" })));
    }

    let allowed = match state
        .rate_limiter
        .check_and_record(
            "rum_ingest",
            &payload.site_key,
            RUM_INGEST_MAX_REQUESTS_PER_SITE,
            RUM_INGEST_WINDOW_SECS,
        )
        .await
    {
        Ok(allowed) => allowed,
        Err(err) => {
            warn!(error = %err, "RUM ingest rate limiter unavailable; failing closed");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            );
        }
    };

    if !allowed {
        warn!("RUM ingest rate limit exceeded for a site");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "status": "rate_limited" })),
        );
    }

    let country_code = state.geo_ip_lookup.lookup_country_code(addr.ip());

    let use_case = IngestRumEventUseCase::new(
        state.rum_site_repository.clone(),
        state.rum_event_repository.clone(),
    );

    match use_case.execute(payload, country_code).await {
        Ok(()) => (StatusCode::ACCEPTED, Json(json!({ "status": "ok" }))),
        Err(IngestRumEventError::UnknownSite) => {
            // Same shape as the well-formed-but-nonexistent case above —
            // never distinguishable from "malformed site_key" by response
            // shape or status code, so this can't be used to enumerate
            // valid site keys.
            (StatusCode::NOT_FOUND, Json(json!({ "status": "rejected" })))
        }
        Err(err @ IngestRumEventError::Validation(_))
        | Err(err @ IngestRumEventError::TooManyJsErrors)
        | Err(err @ IngestRumEventError::TooManyResourceErrors) => {
            warn!(error = %err, "RUM beacon rejected: malformed payload");
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "status": "rejected" })),
            )
        }
        Err(IngestRumEventError::Other(err)) => {
            warn!(error = %err, "failed to persist RUM event");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error" })),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_well_formed_site_keys() {
        assert!(is_well_formed_site_key(
            "rum_0123456789abcdef0123456789abcdef"
        ));
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(!is_well_formed_site_key(
            "xxx_0123456789abcdef0123456789abcdef"
        ));
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(!is_well_formed_site_key("rum_abc"));
    }

    #[test]
    fn rejects_non_hex_characters() {
        assert!(!is_well_formed_site_key(
            "rum_zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
    }
}
