use axum::{
    body::Body,
    extract::Request,
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use domain::RegionCode;

#[derive(Clone, Debug)]
pub struct AgentId(pub String);

#[derive(Clone, Debug)]
pub struct AgentRegion(pub RegionCode);

/// Middleware that authenticates internal agent endpoints.
///
/// This is deliberately separate from `jwt_auth`: agents do NOT present
/// user JWTs. They use a single shared `AGENT_SECRET` (server-to-server)
/// and must also declare their region via `X-Agent-Region` on every
/// claim/submit request.
pub async fn agent_auth(
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let secret = std::env::var("AGENT_SECRET").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = authorization
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if token != secret {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let region_str = headers
        .get("x-agent-region")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let region = RegionCode::new(region_str).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Agent ID is taken from the URL path (`/internal/v1/agents/{agent_id}/...`).
    let path = request.uri().path();
    let segments: Vec<&str> = path.split('/').collect();

    let agent_id = segments
        .iter()
        .position(|segment| *segment == "agents")
        .and_then(|idx| segments.get(idx + 1))
        .map(|s| s.to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    request.extensions_mut().insert(AgentId(agent_id));
    request.extensions_mut().insert(AgentRegion(region));

    Ok(next.run(request).await)
}
