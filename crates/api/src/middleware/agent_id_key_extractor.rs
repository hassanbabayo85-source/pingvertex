//! A [`KeyExtractor`] for `tower_governor` that rate-limits internal agent
//! endpoints by *agent ID* instead of peer/client IP.
//!
//! Internal agent traffic (`/internal/v1/agents/{agent_id}/...`) is
//! server-to-server, not browser-to-server: every agent (e.g.
//! `agent-us-east-1`) polls from behind Docker networking, so under the
//! default `PeerIpKeyExtractor` all agents sharing a network path would be
//! bucketed under the same (or an unstable) IP key — exactly the
//! `caused 429s under normal Docker networking conditions` problem noted
//! at the internal-agent-routes call site in `router.rs`. Keying on the
//! agent ID (parsed the same way `agent_auth::agent_auth` parses it — from
//! the URL path segment after `/agents/`) gives each agent its own
//! independent quota bucket regardless of network topology.
//!
//! This mirrors `agent_auth.rs`'s path-parsing rather than depending on
//! axum's `Path` extractor, because `KeyExtractor::extract` receives a raw
//! `&http::Request<T>` — it runs as a `tower::Layer`, upstream of axum's
//! route-param extraction, so `Path<String>` is not available here.

use axum::http::Request;
use tower_governor::{errors::GovernorError, key_extractor::KeyExtractor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentIdKeyExtractor;

impl KeyExtractor for AgentIdKeyExtractor {
    type Key = String;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        let path = req.uri().path();
        let segments: Vec<&str> = path.split('/').collect();

        segments
            .iter()
            .position(|segment| *segment == "agents")
            .and_then(|idx| segments.get(idx + 1))
            .map(|s| s.to_string())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}
