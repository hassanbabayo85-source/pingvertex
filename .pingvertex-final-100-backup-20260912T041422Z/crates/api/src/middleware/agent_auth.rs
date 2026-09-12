use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_server_mtls::PeerCertificates;
use domain::RegionCode;

#[derive(Clone, Debug)]
pub struct AgentId(pub String);

#[derive(Clone, Debug)]
pub struct AgentRegion(pub RegionCode);

/// Middleware that authenticates internal agent endpoints.
///
/// SEC: this used to also accept a shared `AGENT_SECRET` Bearer token as a
/// fallback whenever `PeerCertificates` was absent, described in a comment
/// as "for tests/legacy". That fallback has been REMOVED ENTIRELY — it was
/// a fully-functional authentication bypass with no cross-check against the
/// requested `{agent_id}` (unlike the mTLS path below), reachable by anyone
/// who obtained the `AGENT_SECRET` value. Even though the specific
/// `main.rs`/`router.rs` wiring at the time meant it wasn't reachable via
/// the plain HTTP listener, "unreachable today because of how three
/// separate things happen to be wired" is not the same guarantee as
/// "cannot happen" — a single future refactor (merging routers, adding a
/// reverse proxy in front of the mTLS port, etc.) would have silently
/// reactivated it. Authentication code should never contain a bypass whose
/// safety depends on infrastructure staying exactly as configured today.
///
/// Identity now comes EXCLUSIVELY from the client certificate's Common Name
/// (CN), injected as a `PeerCertificates` extension by `axum-server-mtls`'s
/// `MtlsAcceptor` after a real mTLS handshake (see `mtls.rs` / the mTLS
/// `axum_server` bound in `main.rs`, which requires every connecting client
/// to present a certificate signed by our internal CA *before* any
/// application code — including this middleware — ever runs). If
/// `PeerCertificates` is missing, this is a hard `UNAUTHORIZED`, full stop
/// — there is no other way to authenticate as an agent.
///
/// Defense-in-depth: the CN is also required to match the `{agent_id}`
/// URL path segment exactly. This isn't needed to establish *whose*
/// certificate was presented — the CN already proves that — but it stops
/// an agent with a valid certificate from acting on another agent's
/// behalf by pointing its URL at a different `{agent_id}`, and it keeps
/// every existing route/handler that reads `AgentId` from the path
/// unchanged.
pub async fn agent_auth(
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();
    let segments: Vec<&str> = path.split('/').collect();
    let path_agent_id = segments
        .iter()
        .position(|segment| *segment == "agents")
        .and_then(|idx| segments.get(idx + 1))
        .map(|s| s.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // mTLS only — no fallback. See doc comment above for why.
    let peer_certs = request
        .extensions()
        .get::<PeerCertificates>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let agent_id = peer_certs
        .leaf_cn()
        .filter(|_| peer_certs.is_present())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if agent_id != path_agent_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let region_str = headers
        .get("x-agent-region")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let region = RegionCode::new(region_str).map_err(|_| StatusCode::BAD_REQUEST)?;

    request.extensions_mut().insert(AgentId(agent_id));
    request.extensions_mut().insert(AgentRegion(region));

    Ok(next.run(request).await)
}
