use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_server_mtls::PeerCertificates;
use domain::AgentStatus;
use domain::RegionCode;

use crate::state::AppState;

/// Request-scoped identity for an authenticated internal agent.
///
/// Every field is populated from the SERVER-SIDE agent registry, not
/// from anything the client sent. The middleware is the only constructor.
///
/// SECURITY: `region` is the value the handlers must use for every
/// security-relevant decision. Client-supplied region claims (headers or
/// payload bodies) are checked against `region` and rejected on mismatch.
#[derive(Clone, Debug)]
pub struct AuthenticatedAgent {
    pub agent_id: String,
    pub region: RegionCode,
    pub capabilities: Vec<String>,
}

/// Middleware that authenticates internal agent endpoints.
///
/// Identity is derived from the client certificate's Common Name (CN),
/// injected as a `PeerCertificates` extension by `axum-server-mtls`'s
/// `MtlsAcceptor` after a real mTLS handshake. If `PeerCertificates` is
/// missing, this is a hard `UNAUTHORIZED` — there is no other way to
/// authenticate as an agent.
///
/// The three-way binding this middleware enforces:
///
///   1. certificate CN         == path {agent_id}
///   2. certificate CN         == a registered agent_id in `agents`
///   3. registered region      == any `X-Agent-Region` header (if sent)
///
/// The **registered region** — never the header — is what the handler
/// receives as `AuthenticatedAgent.region`.
pub async fn agent_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. mTLS-only — no fallback. If the client didn't present a
    //    certificate (or `MtlsAcceptor` didn't run for any reason),
    //    reject. This is the only authentication path.
    let peer_certs = request
        .extensions()
        .get::<PeerCertificates>()
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let cert_agent_id = peer_certs
        .leaf_cn()
        .filter(|_| peer_certs.is_present())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 2. Cert CN must match the URL's {agent_id} segment. This stops
    //    an agent with a valid certificate from acting on another
    //    agent's behalf by pointing its URL at a different agent_id.
    let path = request.uri().path().to_string();
    let segments: Vec<&str> = path.split('/').collect();
    let path_agent_id = segments
        .iter()
        .position(|segment| *segment == "agents")
        .and_then(|idx| segments.get(idx + 1))
        .map(|s| s.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if cert_agent_id != path_agent_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // 3. Server-side registry lookup. This is the authoritative identity
    //    — the region the agent is allowed to operate in comes from
    //    here, NOT from any client-supplied input.
    let agent = state
        .agent_registry
        .find_by_id(&cert_agent_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !matches!(agent.status, AgentStatus::Active) {
        return Err(StatusCode::FORBIDDEN);
    }

    let registered_region =
        RegionCode::new(&agent.region).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 4. Defense-in-depth: if the client sent `X-Agent-Region`, it MUST
    //    match the server-side registered region. A forged header is a
    //    hard rejection — not silently ignored — so operators can see
    //    attempted identity tampering in logs.
    if let Some(header_region) = headers.get("x-agent-region").and_then(|v| v.to_str().ok())
        && header_region != agent.region
    {
        return Err(StatusCode::FORBIDDEN);
    }

    let authenticated = AuthenticatedAgent {
        agent_id: cert_agent_id,
        region: registered_region,
        capabilities: agent.capabilities,
    };

    request.extensions_mut().insert(authenticated);

    Ok(next.run(request).await)
}
