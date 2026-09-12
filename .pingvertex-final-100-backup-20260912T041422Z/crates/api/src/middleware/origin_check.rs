use axum::{
    body::Body,
    http::{
        Request,
        header::{ORIGIN, REFERER},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::error::ApiError;

/// SECURITY (defense-in-depth, audit item #7 "OAuth login-CSRF"):
/// rejects state-changing auth requests (`/auth/login`,
/// `/auth/oauth/google`, `/auth/oauth/apple`, `/auth/reset-password`,
/// `/auth/totp/verify`, ...) whose `Origin` (or, failing that, `Referer`)
/// header does not match this server's own configured origin.
///
/// This is layered ON TOP OF, not instead of, the existing `CorsLayer`
/// (see `router.rs::cors_layer`), which already locks cross-origin
/// `fetch`/XHR access to a single exact `CORS_ALLOWED_ORIGIN`. That
/// already blocks most browser-JS-driven cross-origin CSRF against these
/// JSON endpoints, since a non-simple `Content-Type: application/json`
/// POST triggers a CORS preflight that a foreign origin will fail. This
/// middleware adds an independent, cheaper check that:
///   - catches any future CORS misconfiguration (e.g. an accidental
///     wildcard or overly-broad allow-list) before it becomes a full
///     login-CSRF hole,
///   - and covers request paths that don't go through a browser's CORS
///     preflight logic at all (some non-fetch delivery mechanisms can
///     still carry an `Origin`/`Referer` header even when they bypass
///     CORS enforcement).
///
/// Deliberately permissive when NEITHER header is present: legitimate
/// non-browser clients (native mobile apps, server-to-server calls, curl,
/// most HTTP client libraries) generally do not send `Origin` at all, and
/// rejecting them here would break real logins for no security benefit —
/// the browser-CSRF threat model this exists for is specifically about a
/// victim's BROWSER making the request, which always attaches
/// `Origin`/`Referer` for cross-site requests. Only a *mismatched* value
/// is rejected; an absent one is allowed through to the normal auth
/// checks.
pub async fn require_matching_origin(
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let configured = crate::config::cors_allowed_origin_from_env();

    let header_origin = request
        .headers()
        .get(ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            // Fall back to Referer's origin (scheme://host[:port]) when
            // Origin is absent — some browsers omit Origin on same-site
            // navigations but still send Referer, and a small number of
            // older/edge-case request paths only carry Referer.
            request
                .headers()
                .get(REFERER)
                .and_then(|v| v.to_str().ok())
                .and_then(referer_origin)
        });

    match header_origin {
        None => Ok(next.run(request).await),
        Some(origin) if origin_matches(&origin, &configured) => Ok(next.run(request).await),
        Some(_) => Err(ApiError::forbidden("request origin not allowed").into_response()),
    }
}

/// Extracts `scheme://host[:port]` from a full Referer URL, discarding
/// path/query/fragment.
fn referer_origin(referer: &str) -> Option<String> {
    let scheme_split = referer.find("://")?;
    let after_scheme = &referer[scheme_split + 3..];
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    Some(format!(
        "{}{}",
        &referer[..scheme_split + 3],
        &after_scheme[..authority_end]
    ))
}

fn origin_matches(candidate: &str, configured: &str) -> bool {
    // Exact match, case-insensitive on scheme/host per RFC 6454 (the
    // configured value is a single trusted origin, not a pattern).
    candidate.eq_ignore_ascii_case(configured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referer_origin_strips_path_and_query() {
        assert_eq!(
            referer_origin("https://app.example.com/reset-password?x=1"),
            Some("https://app.example.com".to_string())
        );
    }

    #[test]
    fn origin_matches_is_case_insensitive_on_host() {
        assert!(origin_matches(
            "https://APP.example.com",
            "https://app.example.com"
        ));
    }

    #[test]
    fn origin_matches_rejects_different_origin() {
        assert!(!origin_matches(
            "https://evil.example.com",
            "https://app.example.com"
        ));
    }

    #[test]
    fn origin_matches_rejects_different_scheme_or_port() {
        assert!(!origin_matches(
            "http://app.example.com",
            "https://app.example.com"
        ));
        assert!(!origin_matches(
            "https://app.example.com:8443",
            "https://app.example.com"
        ));
    }
}
