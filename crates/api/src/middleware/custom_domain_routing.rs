use axum::{
    body::Body,
    extract::State,
    http::{Request, header::HOST},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{handlers::public_status_page::render_public_status_page, state::AppState};

/// Routes requests whose `Host` header matches a VERIFIED custom domain
/// straight to that status page's public response, regardless of path.
///
/// This is deliberate vhost-style isolation, not just a routing
/// convenience: once a hostname is recognized as an owner's verified
/// custom domain, that hostname serves ONLY the status page — never any
/// other route on this API, including authenticated endpoints. A request
/// to `status.acme.com/api/v1/monitors` gets the status page response
/// too, exactly like `status.acme.com/anything-else` would.
///
/// SECURITY: the lookup below (`find_by_verified_custom_domain`) is a
/// strict allowlist query — it can only ever match a domain whose
/// ownership was independently confirmed by
/// `CustomDomainVerificationWorker` via a live DNS TXT lookup. There is
/// no pattern-matching, wildcard, or prefix/suffix logic here: the `Host`
/// header (with any port stripped) must equal a `Verified` custom domain
/// value exactly. See the SECURITY notes on `find_by_verified_custom_domain`
/// and `StatusPage::verified_custom_domain` for why this must never be
/// loosened.
pub async fn custom_domain_routing(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let hostname = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(strip_port);

    let Some(hostname) = hostname else {
        return next.run(request).await;
    };

    // Cheap check before touching the database: a bare hostname (no dot)
    // can never be a customer's custom domain — `CustomDomain` validation
    // requires at least one dot (see `domain::value_objects::CustomDomain`)
    // — so this also naturally skips localhost/IP-address requests during
    // local development without an extra query.
    if !hostname.contains('.') {
        return next.run(request).await;
    }

    match state
        .status_page_repository
        .find_by_verified_custom_domain(&hostname)
        .await
    {
        Ok(Some(status_page)) => render_public_status_page(&state, status_page)
            .await
            .into_response(),
        Ok(None) => next.run(request).await,
        Err(err) => {
            // Fail OPEN to normal routing, never to serving stale/wrong
            // status-page content — a database error here should look
            // like "this host isn't a status page", not risk exposing
            // internal routes or the wrong tenant's data.
            tracing::error!(
                error = %err,
                hostname = %hostname,
                "custom domain lookup failed during host-header routing"
            );
            next.run(request).await
        }
    }
}

/// Strips an optional `:port` suffix from a `Host` header value.
///
/// Deliberately simple: real customer custom domains are always regular
/// DNS hostnames (enforced by `CustomDomain` validation at request time),
/// never raw IPv6 literals, so the naive "everything after the last
/// colon" split is safe here — it only needs to handle `example.com` and
/// `example.com:443`, not `[::1]:8080`.
fn strip_port(host: &str) -> String {
    host.rsplit_once(':')
        .map(|(hostname, _port)| hostname)
        .unwrap_or(host)
        .to_lowercase()
}
