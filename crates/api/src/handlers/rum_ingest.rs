use std::net::SocketAddr;

use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{
        HeaderMap, StatusCode,
        header::{ORIGIN, REFERER},
    },
};
use serde_json::{Value, json};
use tracing::warn;

use application::MeterUsageInput;
use application::dto::RumIngestRequest;
use application::ports::RumSiteRepository;
use application::use_cases::IngestRumEventUseCase;

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
const MAX_RUM_REQUEST_ID_BYTES: usize = 128;

/// AUDIT ISSUE #8 fix: per-(site_key, IP) ceiling layered UNDER the
/// per-site limit above, using the same fixed window.
///
/// `site_key` is PUBLIC and non-secret by design (see module docs) —
/// anyone who reads it off a page's source can use it as a rate-limit
/// key. That means the per-site bucket alone can be exhausted by
/// distributed traffic that simply knows a valid site_key, starving
/// real visitors of that site's quota. This does not replace the
/// per-site limit; it bounds how much of that shared budget any single
/// source (by IP) can consume. Set to a fraction of the per-site
/// ceiling so no single source can singlehandedly exhaust a site's
/// whole window under normal traffic, while still comfortably covering
/// one real, busy visitor's tab (a page-view beacon plus a handful of
/// error beacons).
const RUM_INGEST_MAX_REQUESTS_PER_SOURCE: u32 = 40;

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
fn is_well_formed_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RUM_REQUEST_ID_BYTES
        && value.is_ascii()
        && value.bytes().all(|b| b.is_ascii_graphic())
}

fn is_well_formed_site_key(site_key: &str) -> bool {
    match site_key.strip_prefix("rum_") {
        Some(rest) => rest.len() == 32 && rest.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// AUDIT ISSUE #7: returns whether the request's `Origin`/`Referer`
/// header (whichever is present) names the same host as `site_domain`.
/// Absent of BOTH headers, this returns `true` (nothing to compare —
/// see `collect_rum_event`'s doc comment on why this can't be the sole
/// security control) so non-browser or header-stripping paths are never
/// penalized by this advisory check.
fn request_host_matches_site_domain(headers: &HeaderMap, site_domain: &str) -> bool {
    match extract_request_host(headers) {
        Some(host) => host.eq_ignore_ascii_case(site_domain.trim()),
        None => true,
    }
}

/// Extracts just the hostname (no scheme, no port, no path) from the
/// request's `Origin` header, falling back to `Referer` when `Origin`
/// is absent — mirroring `middleware::origin_check`'s same fallback
/// reasoning for why both are checked.
fn extract_request_host(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(host_from_url)
        .or_else(|| {
            headers
                .get(REFERER)
                .and_then(|v| v.to_str().ok())
                .and_then(host_from_url)
        })
}

/// Extracts `host` (no scheme, no port, no path/query/fragment) from a
/// full URL string, e.g. `"https://app.example.com:8443/page?x=1"` ->
/// `"app.example.com"`.
fn host_from_url(value: &str) -> Option<String> {
    let after_scheme = value.split("://").nth(1)?;
    let host_port = after_scheme.split(['/', '?', '#']).next()?;
    let host = host_port.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
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
    headers: HeaderMap,
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

    if !is_well_formed_site_key(&payload.site_key)
        || payload.request_id.as_deref().map(is_well_formed_request_id) == Some(false)
    {
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

    // AUDIT ISSUE #8: second, finer-grained limiter keyed on
    // (site_key, IP) — see `RUM_INGEST_MAX_REQUESTS_PER_SOURCE`'s doc
    // comment for why the per-site limit above isn't sufficient on its
    // own given that site_key is public.
    let per_source_key = format!("{}:{}", payload.site_key, addr.ip());
    let per_source_allowed = match state
        .rate_limiter
        .check_and_record(
            "rum_ingest_source",
            &per_source_key,
            RUM_INGEST_MAX_REQUESTS_PER_SOURCE,
            RUM_INGEST_WINDOW_SECS,
        )
        .await
    {
        Ok(allowed) => allowed,
        Err(err) => {
            warn!(
                error = %err,
                "RUM ingest per-source rate limiter unavailable; failing closed"
            );
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            );
        }
    };

    if !per_source_allowed {
        warn!("RUM ingest rate limit exceeded for a single source against one site");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "status": "rate_limited" })),
        );
    }

    // AUDIT ISSUE #7: advisory Origin/Referer vs `RumSite::domain`
    // allow-list check. Migration 0062's schema comment documents this
    // as the intended purpose of `rum_sites.domain`, but no handler
    // previously implemented it. Deliberately non-blocking: Origin and
    // Referer are client-controlled and are NOT a security boundary
    // (site keys are already public/non-secret — see
    // `is_well_formed_site_key`'s doc comment), and this service may
    // sit behind a proxy/CDN that alters or strips these headers for
    // legitimate traffic. A mismatch is therefore logged as a
    // telemetry-integrity signal, not used to reject the beacon — the
    // authoritative tenant/site resolution remains
    // `RumSiteRepository::find_by_site_key` inside
    // `IngestRumEventUseCase`, unchanged by this check.
    let site = match state
        .rum_site_repository
        .find_by_site_key(&payload.site_key)
        .await
    {
        Ok(Some(site)) => site,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "status": "rejected" }))),
        Err(err) => {
            warn!(error = %err, "RUM site lookup failed during metering");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "status": "error" })),
            );
        }
    };

    if !request_host_matches_site_domain(&headers, &site.domain) {
        warn!("RUM beacon Origin/Referer host did not match the site's configured domain");
    }

    let country_code = state.geo_ip_lookup.lookup_country_code(addr.ip());

    let use_case = IngestRumEventUseCase::new(
        state.rum_site_repository.clone(),
        state.rum_event_repository.clone(),
    );
    let request_id = format!(
        "rum:{}",
        payload
            .request_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
    );

    if let Some(metering) = &state.payg_metering {
        match metering
            .execute(
                MeterUsageInput {
                    tenant_id: site.tenant_id.0,
                    operation: "rum_event".to_owned(),
                    units: 1,
                    request_id,

                    monitor_id: None,
                    execution_id: None,
                    pricing_context: None,
                },
                || use_case.execute_for_site(payload, country_code, site.clone()),
            )
            .await
        {
            Ok(_) => (StatusCode::ACCEPTED, Json(json!({ "status": "ok" }))),
            Err(application::use_cases::ChargeForUsageError::InsufficientBalance) => (
                StatusCode::PAYMENT_REQUIRED,
                Json(json!({ "status": "insufficient_balance" })),
            ),
            Err(err) => {
                warn!(error = %err, "RUM beacon rejected by PAYG metering");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "status": "error" })),
                )
            }
        }
    } else {
        match use_case.execute(payload, country_code).await {
            Ok(()) => (StatusCode::ACCEPTED, Json(json!({ "status": "ok" }))),
            Err(application::use_cases::IngestRumEventError::UnknownSite) => {
                (StatusCode::NOT_FOUND, Json(json!({ "status": "rejected" })))
            }
            Err(err @ application::use_cases::IngestRumEventError::Validation(_))
            | Err(err @ application::use_cases::IngestRumEventError::TooManyJsErrors)
            | Err(err @ application::use_cases::IngestRumEventError::TooManyResourceErrors) => {
                warn!(error = %err, "RUM beacon rejected: malformed payload");
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "status": "rejected" })),
                )
            }
            Err(application::use_cases::IngestRumEventError::Other(err)) => {
                warn!(error = %err, "failed to persist RUM event");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "status": "error" })),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_ascii_request_ids() {
        assert!(is_well_formed_request_id("rum_0123456789abcdef"));
    }

    #[test]
    fn rejects_oversized_or_non_ascii_request_ids() {
        assert!(!is_well_formed_request_id(
            &"x".repeat(MAX_RUM_REQUEST_ID_BYTES + 1)
        ));
        assert!(!is_well_formed_request_id("rum-ƙey"));
    }

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

    #[test]
    fn host_from_url_strips_scheme_port_and_path() {
        assert_eq!(
            host_from_url("https://app.example.com:8443/page?x=1"),
            Some("app.example.com".to_string())
        );
        assert_eq!(
            host_from_url("http://example.com"),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn host_from_url_rejects_malformed_input() {
        assert_eq!(host_from_url("not-a-url"), None);
    }

    #[test]
    fn request_host_matches_site_domain_true_when_no_headers_present() {
        let headers = HeaderMap::new();
        assert!(request_host_matches_site_domain(&headers, "example.com"));
    }

    #[test]
    fn request_host_matches_site_domain_compares_case_insensitively() {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, "https://App.Example.com".parse().unwrap());
        assert!(request_host_matches_site_domain(
            &headers,
            "app.example.com"
        ));
    }

    #[test]
    fn request_host_matches_site_domain_detects_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, "https://evil.example".parse().unwrap());
        assert!(!request_host_matches_site_domain(
            &headers,
            "app.example.com"
        ));
    }
}
